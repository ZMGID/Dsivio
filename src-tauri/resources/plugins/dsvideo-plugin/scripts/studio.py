#!/usr/bin/env python3
"""Dsivio video workspace. JSON stdin/stdout; shared by the desktop and chat skills.

No credentials in task files. Submission is persisted BEFORE the network call;
an uncertain submission is never automatically repeated.
"""
import argparse
import importlib.util
import hashlib
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid
from urllib.parse import urljoin, urlsplit

from dsvideo_config import get_provider, save_provider, normalize_base_url, config_path
import runtime
from model_catalog import video_quote

PLUGIN = Path(__file__).resolve().parents[1]
DATA_HOME = Path(os.environ.get('APPDATA') or (Path.home() / 'Library/Application Support' if sys.platform == 'darwin' else os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share'))
ROOT = Path(os.environ.get('DSVIDEO_STUDIO_ROOT') or DATA_HOME / 'com.zmair.kivio' / 'video-studio')


def submission_failure(error):
    status = getattr(error, 'http_status', None)
    reasons = {
        400: '服务不接受当前请求，请检查模型、素材和生成规格。',
        401: 'API Key 无效或已过期，请在视频设置中检查密钥。',
        402: '服务拒绝计费，请检查供应商余额或套餐。',
        403: '没有使用该模型的权限，请检查供应商授权。',
        404: '生成接口或模型不存在，请检查服务地址和模型名称。',
        405: '服务地址不支持此生成接口，请检查接口配置。',
        413: '素材或请求过大，请压缩素材后重试。',
        415: '服务不支持当前素材格式，请更换素材。',
        422: '服务不接受当前参数，请检查素材、时长和清晰度。',
        429: '服务正在限流，请稍后重试。',
    }
    rejected = status in reasons or bool(getattr(error, 'not_submitted', False))
    reason = reasons.get(status, '连接未建立，请检查服务地址和网络后重试。') if rejected else (
        '服务返回异常，尚不能确认是否已接单。暂不重复提交，避免重复生成。' if status else
        '没有收到可识别的任务编号，可能是响应格式不兼容或连接中断。暂不重复提交，避免重复生成。')
    return {'state': 'rejected' if rejected else 'uncertain', 'httpStatus': status,
            'reason': reason, 'retryable': rejected}


def module(name, folder):
    spec = importlib.util.spec_from_file_location(name, PLUGIN / 'skills' / folder / 'scripts' / (name + '.py'))
    value = importlib.util.module_from_spec(spec)
    sys.modules[name] = value
    spec.loader.exec_module(value)
    return value


grok = module('grok_video', 'grok-video-api')
mini = module('minimax_h3', 'minimax-h3-api')


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix('.' + uuid.uuid4().hex + '.tmp')
    tmp.write_text(json.dumps(value, ensure_ascii=False, indent=2), encoding='utf-8')
    os.replace(tmp, path)


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def task_path(id):
    return ROOT / 'tasks' / (str(uuid.UUID(id)) + '.json')


def persist(t):
    t['revision'] = t.get('revision', 0) + 1
    t['updatedAt'] = int(time.time() * 1000)
    write(task_path(t['id']), t)
    return t


def settings():
    return {name: {**{k: v for k, v in get_provider(name).items() if k in ('base_url', 'model')},
                   'ready': bool(get_provider(name).get('api_key'))}
            for name in ('grok', 'minimax', 'comfy')}


def import_brief(brief):
    b = dict(brief)
    images = []
    for source in b.get('images', []):
        p = Path(source)
        if p.suffix.lower() not in ('.jpg', '.jpeg', '.png', '.webp') or p.stat().st_size > 30 * 1024 * 1024:
            raise ValueError('参考图需为 PNG/JPG/WebP 且每张不超过 30 MB')
        raw = p.read_bytes()
        dest = ROOT / 'assets' / (hashlib.sha256(raw).hexdigest() + p.suffix.lower())
        if not dest.exists():
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(raw)
        images.append(str(dest))
    b['images'] = images
    for field in ('firstFrame', 'lastFrame'):
        value = b.get(field)
        if value:
            if value not in brief.get('images', []):
                raise ValueError('首尾帧必须从已添加的图片中选择')
            b[field] = images[brief['images'].index(value)]
    for field, extensions, limit in [('referenceVideos', ('.mp4', '.mov'), 50), ('referenceAudios', ('.mp3', '.wav'), 15)]:
        copied = []
        seconds = 0.0
        paths = b.get(field, [])
        if len(paths) > 3:
            raise ValueError('参考视频和音频分别最多 3 个')
        for path in paths:
            p = Path(path)
            if p.suffix.lower() not in extensions or p.stat().st_size > limit * 1024 * 1024:
                raise ValueError(f'参考媒体格式不支持或超过 {limit} MB')
            try:
                result = subprocess.run(['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'json', str(p)], capture_output=True, text=True, timeout=20)
                duration = float(json.loads(result.stdout)['format']['duration'])
            except Exception:
                raise ValueError('无法读取参考媒体时长，请检查 FFmpeg / ffprobe 安装')
            if not 2 <= duration <= 15:
                raise ValueError('每段参考视频和音频必须为 2–15 秒')
            seconds += duration
            raw = p.read_bytes()
            dest = ROOT / 'assets' / (hashlib.sha256(raw).hexdigest() + p.suffix.lower())
            dest.parent.mkdir(parents=True, exist_ok=True)
            if not dest.exists():
                dest.write_bytes(raw)
            copied.append(str(dest))
        if seconds > 15.01:
            raise ValueError('参考视频和参考音频各自总时长不得超过 15 秒')
        b[field] = copied
        b[field + 'Seconds'] = seconds
    return b


def bootstrap():
    for folder in ('tasks', 'templates', 'outputs'):
        (ROOT / folder).mkdir(parents=True, exist_ok=True)
    target = ROOT / 'templates' / 'bedroom-ugc-product-presenter-15s.json'
    if not target.exists():
        value = read(PLUGIN / 'skills/ecom-h3-video/templates/bedroom-ugc-product-presenter-15s.json')
        value['kind'] = 'generation'
        write(target, value)
    def listing(folder):
        result = []
        for path in (ROOT / folder).glob('*.json'):
            try:
                result.append(read(path))
            except (ValueError, OSError):
                pass
        return result
    return {'tasks': sorted(listing('tasks'), key=lambda t: t.get('updatedAt', 0), reverse=True),
            'templates': listing('templates'), 'config': settings(), 'root': str(ROOT),
            'configPath': str(config_path()), 'dependencies': runtime.status()}


def validate(t):
    b = t['brief']
    route = b.get('route')
    if route not in ('grok', 'minimax', 'comfy'):
        raise ValueError('请明确选择生成路线')
    count = len(b.get('images', []))
    mode = b.get('inputMode', 'auto')
    if mode == 'auto':
        mode = 'reference' if route == 'grok' and (count > 1 or b.get('voiceIds')) else 'image' if route == 'grok' and count else 'reference' if count or b.get('referenceVideos') else 'text'
    if mode not in ('text', 'image', 'reference', 'frames'):
        raise ValueError('未知生成模式')
    if mode == 'text' and (count or b.get('referenceVideos') or b.get('referenceAudios') or b.get('voiceIds')):
        raise ValueError('文生视频模式不使用参考素材，请移除素材或切换模式')
    if mode == 'frames' and route != 'minimax':
        raise ValueError('当前首尾帧模式仅支持 MiniMax')
    if route != 'minimax' and (b.get('referenceVideos') or b.get('referenceAudios') or b.get('firstFrame') or b.get('lastFrame')):
        raise ValueError('当前路线不支持 MiniMax 首尾帧或参考音视频，请移除这些素材')
    if route != 'grok' and b.get('voiceIds'):
        raise ValueError('预设音色仅适用于 Grok 参考生成')
    if len(b.get('voiceIds', [])) > 3 or (b.get('voiceIds') and b.get('speechMode') == 'silent'):
        raise ValueError('Grok 最多选择 3 个音色；静音时请清空音色选择')
    if b.get('voiceIds') and mode != 'reference':
        raise ValueError('预设音色需要参考生成模式')
    if count > {'grok': 7 if mode == 'reference' else 1, 'minimax': 9, 'comfy': 3}[route]:
        raise ValueError('当前路线不支持这么多参考图，请调整素材或路线')
    if mode == 'image' and count != 1:
        raise ValueError('单图模式需要恰好一张图片')
    if route == 'grok' and mode == 'reference' and not count and not b.get('voiceIds'):
        raise ValueError('参考生成至少需要一张图片或一个预设音色')
    if route == 'grok' and mode == 'reference' and b['resolution'] == '1080p':
        raise ValueError('Grok 参考生成最高支持 720p')
    if mode == 'frames':
        frames = [v for v in (b.get('firstFrame'), b.get('lastFrame')) if v]
        if not frames or set(b.get('images', [])) != set(frames) or b.get('referenceVideos') or b.get('referenceAudios'):
            raise ValueError('首尾帧模式仅保留选中的首尾帧图片，不能混用其他参考素材')
    if b.get('ratio') not in {'grok': grok.RATIOS, 'minimax': mini.RATIOS, 'comfy': ('1:1','2:3','3:2','3:4','4:3','9:16','16:9','21:9')}[route]:
        raise ValueError('所选画幅不适用于当前路线')
    if mode != 'frames' and (b.get('firstFrame') or b.get('lastFrame')):
        raise ValueError('请切换到首尾帧模式或清空首尾帧选择')
    if len(b.get('images', [])) + len(b.get('referenceVideos', [])) + len(b.get('referenceAudios', [])) > 12:
        raise ValueError('参考媒体总数不得超过 12 个')
    if b.get('referenceAudios') and not (count or b.get('referenceVideos')):
        raise ValueError('参考音频需要同时提供图片或视频')
    b = {**b, 'effectiveMode': mode}
    low = {'grok': 1, 'minimax': 4, 'comfy': 2}[route]
    if not low <= int(b['duration']) <= 15:
        raise ValueError(f'当前路线支持 {low}–15 秒')
    allowed = {'grok': grok.RESOLUTIONS, 'minimax': mini.RESOLUTIONS, 'comfy': ['0.5', '1']}[route]
    if b.get('resolution') not in allowed:
        raise ValueError('请选择此路线支持的清晰度')
    return b, route


def client(route, snapshot=None):
    p = get_provider(route)
    base = (snapshot or p).get('base_url') or ('https://api.x.ai' if route == 'grok' else 'https://api.minimaxi.com')
    if snapshot and normalize_base_url(base) != normalize_base_url(p.get('base_url') or base):
        raise ValueError('供应商地址已改变，请恢复原地址后查询此任务')
    if not p.get('api_key'):
        raise ValueError('请先在视频设置中配置此路线的 API Key')
    cls = grok.GrokVideoClient if route == 'grok' else mini.MiniMaxClient
    return cls(base, p['api_key'])


def request(t):
    b, route = validate(t)
    args = dict(prompt=t['prompt'], duration=int(b['duration']), resolution=b['resolution'], ratio=b['ratio'])
    if route == 'grok':
        refs = b['effectiveMode'] == 'reference'
        return grok.build_video_request(**args, image=next(iter(b.get('images', [])), None) if not refs else None,
                                       reference_images=b.get('images', []) if refs else [], voice_ids=b.get('voiceIds', []) if refs else [],
                                       generate_audio=b.get('speechMode') != 'silent',
                                       model=get_provider(route).get('model') or grok.MODEL)
    frames = b['effectiveMode'] == 'frames'
    return mini.build_video_request(**args, first_frame=b.get('firstFrame') if frames else None,
                                   last_frame=b.get('lastFrame') if frames else None,
                                   reference_images=[] if frames else b.get('images', []),
                                   reference_videos=b.get('referenceVideos', []), reference_audios=b.get('referenceAudios', []))


def handle(action, data):
    if action == 'install_comfy':
        return runtime.install()
    if action == 'bootstrap':
        return bootstrap()
    if action == 'config':
        name = data['name']
        if name not in ('grok', 'minimax', 'comfy'):
            raise ValueError('未知路线')
        p = get_provider(name)
        p['base_url'] = normalize_base_url(data['base_url'])
        p['studio_revision'] = str(uuid.uuid4())
        if data.get('api_key'):
            p['api_key'] = data['api_key']
        if name == 'grok':
            p['model'] = data.get('model') or grok.MODEL
        save_provider(name, p)
        return settings()
    if action == 'template_import':
        value = read(Path(data['path']))
        if not value.get('name') or not (value.get('shots') or value.get('script')):
            raise ValueError('模板需要 name 和 shots 或 script')
        value['id'] = str(uuid.uuid4())
        value['kind'] = data.get('kind', 'reference')
        write(ROOT / 'templates' / (value['id'] + '.json'), value)
        return value
    if action == 'create':
        bootstrap()
        return persist({'id': str(uuid.uuid4()), 'brief': import_brief(data['brief']), 'script': '', 'prompt': '',
                        'status': 'draft', 'approved': False})
    t = read(task_path(data['id']))
    if action == 'get':
        return t
    if data.get('revision') != t['revision']:
        raise ValueError('任务已在其他窗口更新，请重新打开任务')
    if action in ('save', 'plan_result', 'approve', 'prompt_result', 'quote', 'analysis_result'):
        if t['status'] in ('submitting', 'running', 'uncertain'):
            raise ValueError('生成尚未结束，请等待完成后再改')
    if action == 'save':
        t.update(brief=import_brief(data['brief']), script=data.get('script', ''), concepts=[], approved=False, prompt='', quote=None, status='draft')
        t.pop('remote', None)
    elif action in ('plan_result', 'analysis_result'):
        t.update(script=data['script'], concepts=data.get('concepts', []), approved=False, prompt='', quote=None, status='draft')
        t.pop('remote', None)
        if action == 'analysis_result':
            t['analysis'] = data.get('analysis')
    elif action == 'approve':
        if not t['script'].strip():
            raise ValueError('请先完成剧本')
        validate(t)
        t.update(approved=True, status='approved', prompt='', quote=None)
    elif action == 'prompt_result':
        if not t['approved']:
            raise ValueError('需要先确认当前剧本')
        t['prompt'] = data['prompt']
    elif action == 'quote':
        b, route = validate(t)
        if not t.get('prompt') or not t['approved']:
            raise ValueError('请先确认剧本并转换提示词')
        if route == 'comfy':
            t['quote'] = {'note': '本地工作流；算力与工作流节点费用取决于你的 ComfyUI 配置。', 'base_url': get_provider('comfy').get('base_url') or 'http://127.0.0.1:8188'}
        else:
            provider = get_provider(route)
            model = (provider.get('model') or grok.MODEL) if route == 'grok' else 'MiniMax-H3'
            q = video_quote(model, int(b['duration']), len(b['images']), b.get('referenceVideosSeconds', 0))
            if route == 'grok' and b['effectiveMode'] == 'reference':
                q.get('estimated_cost', {}).pop('1080p', None)
            q['base_url'] = provider.get('base_url') or ('https://api.x.ai' if route == 'grok' else 'https://api.minimaxi.com')
            if b.get('referenceVideos') and not b.get('referenceVideosSeconds'):
                q['note'] += ' 参考视频输入费用未计入。'
            t['quote'] = q
        t['quote']['at'] = time.time()
        t['quote']['providerRevision'] = get_provider(route).get('studio_revision')
        t['quote']['model'] = get_provider(route).get('model')
    elif action == 'submit':
        b, route = validate(t)
        if t['status'] in ('submitting', 'running', 'uncertain'):
            raise ValueError('任务已经提交，请查看结果或继续查询')
        if not t.get('prompt'):
            raise ValueError('请先填写或生成视频提示词')
        t['remote'] = {'route': route, 'base_url': get_provider(route).get('base_url') or
                       {'grok': 'https://api.x.ai', 'minimax': 'https://api.minimaxi.com', 'comfy': 'http://127.0.0.1:8188'}[route]}
        if route == 'comfy':
            t['status'] = 'submitting'
            return persist(t)
        c = client(route, t['remote'])
        payload = request(t)
        if len(json.dumps(payload).encode('utf-8')) > 64 * 1024 * 1024:
            raise ValueError('请求超过 64 MB，请压缩或减少参考素材')
        t['requested'] = {k: payload[k] for k in ('duration', 'resolution', 'model') if k in payload}
        t['status'] = 'submitting'
        t.pop('error', None)
        t.pop('submission', None)
        persist(t)
        try:
            t['remote']['id'] = c.create_video(payload)
            t['status'] = 'running'
        except Exception as error:
            failure = submission_failure(error)
            t['submission'] = failure
            t['error'] = failure['reason']
            t['status'] = 'approved' if failure['retryable'] else 'uncertain'
            if failure['retryable']:
                t.pop('remote', None)
        return persist(t)
    elif action == 'comfy_workflow':
        if t['status'] != 'submitting' or t['remote']['route'] != 'comfy':
            raise ValueError('无待提交的 ComfyUI 任务')
        m = module('prepare_workflow', 'ecom-h3-video')
        flow = m.prepare_workflow(read(PLUGIN / 'skills/ecom-h3-video/assets/minimax-h3-workflow.json'),
                                  prompt=t['prompt'], duration=int(t['brief']['duration']), ratio=t['brief']['ratio'],
                                  images=data['images'], megapixels=float(t['brief']['resolution']))
        path = ROOT / 'outputs' / t['id'] / 'workflow.json'
        write(path, flow)
        return {'path': str(path)}
    elif action == 'comfy_submitted':
        if t['status'] != 'submitting':
            raise ValueError('任务已经提交')
        t['remote']['id'] = data['remoteId']
        t['status'] = 'running'
    elif action == 'uncertain':
        t['status'] = 'uncertain'
        t['error'] = '未获得可靠提交回执，请核查 ComfyUI 队列后处理；不会自动重新生成。'
        if data.get('detail'):
            t['error'] += '\n' + data['detail']
    elif action == 'preflight_failed':
        if t['status'] != 'submitting' or t.get('remote', {}).get('id'):
            raise ValueError('无法回退已提交任务')
        t.update(status='approved', error='素材或工作流准备失败，请检查 Comfy MCP 依赖和服务地址。本次未提交生成。')
        if data.get('detail'):
            t['error'] += '\n' + data['detail']
    elif action == 'recover':
        if t['status'] not in ('uncertain', 'submitting') or not t.get('remote'):
            raise ValueError('此任务不需要补录编号')
        remote_id = str(data.get('remoteId', '')).strip()
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,160}', remote_id):
            raise ValueError('请输入控制台中的有效任务编号')
        t['remote']['id'] = remote_id
        t.update(status='running', error='')
    elif action == 'poll':
        if not t.get('remote', {}).get('id'):
            raise ValueError('没有远程任务编号，不能恢复查询')
        r = t['remote']
        if r['route'] == 'comfy':
            return t
        c = client(r['route'], r)
        result = c.get_video(r['id']) if r['route'] == 'grok' else c.get_task(r['id'])
        status = result.get('status')
        if status in ('done', 'succeeded'):
            m = grok if r['route'] == 'grok' else mini
            url = (result.get('video') if r['route'] == 'grok' else result.get('content') or {}).get('url')
            if not url:
                raise ValueError('生成完成但服务没有返回视频地址')
            dest = ROOT / 'outputs' / t['id'] / 'video.mp4'
            download_url = urljoin(c.base_url + '/', url)
            if r['route'] == 'grok':
                same_origin = urlsplit(download_url)[:2] == urlsplit(c.base_url)[:2]
                m.download_video(download_url, dest, api_key=c.api_key if same_origin else None)
            else:
                m.download_video(download_url, dest)
            t.update(status='succeeded', output=str(dest))
        elif status in ('failed', 'cancelled', 'expired', 'error'):
            t.update(status='failed', error='供应商任务失败：' + str(status))
        return persist(t)
    elif action in ('comfy_complete', 'comfy_failed'):
        if action == 'comfy_complete':
            t.update(status='succeeded', output=data['output'])
        else:
            t.update(status='failed', error='ComfyUI 返回任务失败或取消，请检查服务端执行记录。')
        try:
            t['cleanup'] = module('free_comfy_memory', 'ecom-h3-video').free_comfy_memory(t['remote']['base_url'])
        except Exception:
            t['cleanup'] = {'status': '无法释放显存，请检查服务器；成片已保留'}
    elif action == 'template_save':
        kind = 'reference' if t['brief'].get('mode') == 'analysis' else 'generation'
        if kind == 'generation' and (t['status'] != 'succeeded' or not data.get('approvedOutput')):
            raise ValueError('生成模板须在确认成片后保存')
        if not t['script'].strip():
            raise ValueError('没有可保存的剧本')
        value = {'id': str(uuid.uuid4()), 'name': data['name'], 'kind': kind, 'script': t['script'],
                 'spec': {'duration_seconds': t['brief']['duration'], 'aspect_ratio': t['brief']['ratio']} if kind == 'generation' else {}}
        write(ROOT / 'templates' / (value['id'] + '.json'), value)
        return value
    else:
        raise ValueError('未知操作：' + action)
    return persist(t)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action')
    args = parser.parse_args()
    ROOT.mkdir(parents=True, exist_ok=True)
    data = json.load(sys.stdin)
    # Only operations on the same task need serialization. A slow provider must
    # not block bootstrap, settings, or progress of every other video task.
    lock_path = ROOT / '.lock'
    if data.get('id'):
        lock_path = task_path(data['id']).with_suffix('.lock')
        lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open('a+b') as lock:
        lock.seek(0)
        if os.name == 'nt':
            import msvcrt
            if lock.read(1) == b'':
                lock.write(b'0')
                lock.flush()
            lock.seek(0)
            msvcrt.locking(lock.fileno(), msvcrt.LK_LOCK, 1)
        else:
            import fcntl
            fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            print(json.dumps(handle(args.action, data), ensure_ascii=False))
        finally:
            if os.name == 'nt':
                lock.seek(0)
                msvcrt.locking(lock.fileno(), msvcrt.LK_UNLCK, 1)


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        # Providers may include request data in exceptions; never echo API errors/keys.
        print(json.dumps({'error': str(exc) if isinstance(exc, ValueError) else
                          '视频操作失败，请检查配置、依赖和网络。远程任务可通过任务编号继续查询。'}, ensure_ascii=False))
        sys.exit(1)
