"""Agent-supervised TK collection: fixed export steps, no browser recovery/close.

This produces a capture, not a rendered daily report. One output root/date/store
is one export attempt; reruns resume it and never silently submit another export.
"""
import argparse
from datetime import date, datetime, timedelta, timezone
from decimal import Decimal
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from download_files import archive, digest
from report_state import read_json, write_json
from tiktok_export import number, parse_export
from agent_handoff import handoff

HERE = Path(__file__).resolve().parent
URL = 'https://seller-br.tiktok.com/compass/data-overview?shop_region=BR'


def now():
    return datetime.now().astimezone().isoformat(timespec='seconds')


def log(message):
    print(f'[{now()}] {message}', flush=True)


def cli(*args, structured=True):
    env = dict(os.environ, PYTHONIOENCODING='utf-8')
    result = subprocess.run([sys.executable, str(HERE / 'ziniao_cli.py'), *map(str, args)],
                            capture_output=True, encoding='utf-8', env=env, timeout=60)
    if result.returncode:
        # Do not persist arbitrary CLI errors, which may contain session URLs.
        raise RuntimeError(f'紫鸟命令失败: {args[0]} {args[1]} (exit={result.returncode})')
    if not structured:
        confirmed = ('店铺已关闭' if args[:2] == ('store', 'close') else
                     '页面已导航' if args[:2] == ('page', 'visit') else 'targetMatched=true')
        if confirmed not in result.stdout + result.stderr:
            raise RuntimeError('点击未确认命中目标；不能自动重复提交导出')
        return None
    data = json.loads(result.stdout)
    while isinstance(data, dict) and 'data' in data:
        if data.get('ok') is False:
            raise RuntimeError('紫鸟返回失败状态')
        data = data['data']
    if isinstance(data, dict) and data.get('ok') is False:
        raise RuntimeError('紫鸟返回失败状态')
    return data


def observe(store, target):
    data = cli('page', 'exec', '--store-id', store, '--target-id', target,
               '--script-file', HERE / 'tiktok_observe.js')
    if data.get('exceptionDetails'):
        raise RuntimeError('页面观察脚本异常')
    return json.loads(data['result'])


def click(store, target, selector):
    log(f'点击 {selector}')
    try:
        cli('page', 'click', '--store-id', store, '--target-id', target,
            '--selector', selector, structured=False)
    except Exception:
        script = """(() => JSON.stringify((() => {
          const selector = %s;
          const nodes = [...document.querySelectorAll(selector)].filter((el) => {
            const style = getComputedStyle(el), rect = el.getBoundingClientRect();
            return style.display !== 'none' && style.visibility !== 'hidden' &&
              rect.width > 0 && rect.height > 0 && !el.disabled;
          });
          if (nodes.length !== 1) return {ok:false, count:nodes.length};
          nodes[0].click();
          return {ok:true, count:1};
        })()))()""" % json.dumps(selector)
        reply = cli('page', 'exec', '--store-id', store, '--target-id', target,
                    '--script', script)
        result = reply.get('result', {})
        if isinstance(result, str):
            result = json.loads(result)
        if reply.get('exceptionDetails') or result.get('ok') is not True:
            raise ValueError('页面内点击未确认: '+selector)


def verify_page(state, account):
    if state['url'] != URL.split('?')[0] or not state['exportVisible']:
        raise ValueError('未进入关键指标页：等待加载，或需处理登录/验证/代理问题')
    if account not in [line.strip() for line in state['text'].splitlines()]:
        raise ValueError('页面账号未与预期账号匹配')
    if not state['currencyBRL'] or number(state['sales']) is None or number(state['orders']) is None:
        raise ValueError('关键指标或 BRL 币种尚未就绪')
    if not state.get('start') or not state.get('end'):
        raise ValueError('页面日期输入框尚未加载')
    start, end = [date.fromisoformat(state[key].replace('/', '-')) for key in ('start', 'end')]
    if start > end:
        raise ValueError('页面日期区间无效')
    return start, end


def ready(store, target, account, seconds, target_date=None):
    until, previous, reason = time.monotonic() + seconds, None, '页面未就绪'
    while time.monotonic() < until:
        try:
            # Launch/login redirects may replace the initial tab target.
            target = cli('page', 'content', '--store-id', store)['targetId']
            state = observe(store, target)
            start, end = verify_page(state, account)
            if target_date and not (start <= date.fromisoformat(target_date) <= end and (end-start).days == 6):
                raise ValueError('七天区间尚未刷新或不覆盖目标日')
            signature = (state['start'], state['end'], state['sales'], state['orders'])
            if signature == previous:
                state['_target'] = target
                return state
            previous = signature
        except (ValueError, RuntimeError, TypeError) as error:
            reason, previous = str(error), None
        time.sleep(5)
    raise ValueError(reason)


def files(folder):
    return {str(path): [path.stat().st_size, path.stat().st_mtime_ns]
            for path in folder.glob('*.xlsx') if path.is_file()}


def wait_file(folder, baseline, seconds):
    until, previous, last_log = time.monotonic() + seconds, {}, 0
    while time.monotonic() < until:
        current = files(folder)
        new = [Path(path) for path, stat in current.items()
               if stat[0] > 0 and baseline.get(path) != stat and previous.get(path) == stat]
        if len(new) > 1:
            raise ValueError('本店出现多份新增文件，来源有歧义，需核验同一次导出')
        if len(new) == 1:
            return new[0]
        previous = current
        if time.monotonic() - last_log >= 30:
            log('等待同一次导出的自动下载，不重复点击导出')
            last_log = time.monotonic()
        time.sleep(5)
    return None


def serializable(value):
    return json.loads(json.dumps(value, ensure_ascii=False, default=lambda v: str(v) if isinstance(v, Decimal) else v.isoformat()))


def collect(args):
    root = Path(args.output_root).resolve()
    folder = root / '数据' / args.date / 'TK采集' / args.store_id
    folder.mkdir(parents=True, exist_ok=True)
    checkpoint, capture = folder / 'checkpoint.json', folder / 'capture.json'
    lock = folder / 'running.lock'
    # A stale lock is intentionally not stolen; an agent first checks its owner.
    with lock.open('x', encoding='utf-8') as handle:
        handle.write(str(os.getpid()))
    state = {}
    try:
        identity = {'storeId': args.store_id, 'name': args.expected_name,
                    'account': args.expected_account, 'dateIso': args.date}
        state = read_json(checkpoint) if checkpoint.exists() else {'identity': identity, 'stage': 'new'}
        if state['identity'] != identity:
            raise ValueError('断点的店铺、账号或日期与本次参数不一致')
        if state['stage'] in ('success', 'collected'):
            saved = read_json(capture)
            if digest(Path(saved['archive']['path'])) != saved['archive']['sha256']:
                raise ValueError('已完成结果的归档文件缺失或发生变化')
            log('复用已核验结果，不打开店铺、不重复导出')
            if getattr(args, 'collect_ads', False):
                from tiktok_ads import collect as collect_ads
                return collect_ads(args, capture)
            return saved
        download = Path(args.download_dir).resolve(strict=True)
        target = cli('page', 'content', '--store-id', args.store_id)['targetId']
        if state['stage'] in ('new', 'needs_review'):
            initial = ready(args.store_id, target, args.expected_account, args.ready_wait)
            target = initial['_target']
            try:
                initial_start = date.fromisoformat(initial['start'].replace('/', '-'))
                initial_end = date.fromisoformat(initial['end'].replace('/', '-'))
                target_day = date.fromisoformat(args.date)
                range_ready = (initial_end - initial_start).days == 6 and initial_start <= target_day <= initial_end
            except (AttributeError, TypeError, ValueError):
                range_ready = False
            if not range_ready:
                if initial['historyVisible']:
                    click(args.store_id, target, '[data-testid="export-history-button"]')
                click(args.store_id, target, '[data-tid="m4b_date_picker_range_picker"]')
                click(args.store_id, target, '[data-testid="time-selector-last-7-days"]')
            page = ready(args.store_id, target, args.expected_account, args.ready_wait, args.date)
            target = page['_target']
        if state['stage'] == 'needs_review':
            if any(page[key] != state['page'][key] for key in ('start', 'end')):
                raise ValueError('复核窗口已变化，不用新窗口对账旧导出')
            state['page'] = {key: page[key] for key in ('url', 'start', 'end', 'sales', 'orders', 'currencyBRL')}
            write_json(checkpoint, state)
            log('只复核同区间页面和已归档文件，不重复导出')
        if state['stage'] == 'new':
            if not page['historyVisible']:
                click(args.store_id, target, '[data-testid="export-history-button"]')
            history = observe(args.store_id, target)['history']
            click(args.store_id, target, '[data-testid="export-history-button"]')
            state.update(stage='waiting_export', submittedAt=now(), baseline=files(download),
                         downloadDir=str(download), historyBefore=history,
                         page={key: page[key] for key in ('url', 'start', 'end', 'sales', 'orders', 'currencyBRL')})
            # Intent must be durable BEFORE the click: ambiguous response never causes a second export.
            write_json(checkpoint, state)
            click(args.store_id, target, '[data-testid="export-button"]')
            log('已提交一次七天关键指标导出')
        elif state['downloadDir'] != str(download):
            raise ValueError('恢复时店铺下载目录发生变化')

        if capture.exists() and read_json(capture).get('downloads'):
            archive_entry = read_json(read_json(capture)['downloads'][-1])
            source = Path(archive_entry['path'])
            if digest(source) != archive_entry['sha256']:
                raise ValueError('归档文件校验失败')
        else:
            source = wait_file(download, state['baseline'], args.max_wait)
            if source is None:
                raise TimeoutError('已提交，下载尚未落盘；Agent 核对同次导出历史，续跑不重复提交')
            start, end = (state['page'][key].replace('/', '-') for key in ('start', 'end'))
            # Confirm source metadata before transferring; parse authoritative archived copy below.
            parse_export(source, args.date, start, end)
            write_json(capture, {'dateIso': args.date, 'configSnapshot': {
                'report': {'outputRoot': str(root)}, 'stores': [{'storeId': args.store_id}]}, 'downloads': []})
            archive_entry = archive(capture, args.store_id, source, download, '关键指标', start, end)
            source = Path(archive_entry['path'])
        page = state['page']
        parsed = parse_export(source, args.date, page['start'].replace('/', '-'), page['end'].replace('/', '-'))
        for metric, output in (('GMV', 'sales'), ('订单数', 'orders')):
            if number(page[output]) != parsed['totals'][metric]:
                parsed['missingReason'] += f'；{metric}页面区间值与文件总计不一致，需单日复核'
        result = {**read_json(capture), **identity, **serializable(parsed), 'archive': archive_entry,
                  'page': page, 'capturedAt': now(),
                  'timeZone': 'America/Sao_Paulo', 'timeZoneSource': 'BR 站点配置，非页面检测',
                  'status': 'needs_agent' if parsed['missingReason'] else 'collected'}
        state['stage'] = 'needs_review' if parsed['missingReason'] else 'collected'
        write_json(capture, result)
        write_json(checkpoint, state)
        write_json(folder / 'handoff.json', {**identity, 'status': result['status'], 'reason': 'Agent 核验 capture 后决定采用及关店'})
        if result['status'] == 'collected' and getattr(args, 'collect_ads', False):
            from tiktok_ads import collect as collect_ads
            return collect_ads(args, capture)
        return result
    except Exception as error:
        return handoff(folder, identity, error)
    finally:
        lock.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--store-id', required=True)
    parser.add_argument('--expected-name', required=True)
    parser.add_argument('--expected-account', required=True)
    parser.add_argument('--date', default=(datetime.now(timezone(timedelta(hours=-3))).date()-timedelta(days=1)).isoformat())
    parser.add_argument('--output-root', required=True)
    parser.add_argument('--ready-wait', type=int, default=90)
    parser.add_argument('--max-wait', type=int, default=600)
    parser.add_argument('--download-dir', required=True, help='Agent 从本店开店返回值获取的下载目录')
    parser.add_argument('--collect-ads', action='store_true')
    args = parser.parse_args()
    date.fromisoformat(args.date)
    if not args.store_id.isdigit() or args.max_wait < 10 or args.ready_wait < 10:
        parser.error('店铺 ID 必须为数字，等待时间至少 10 秒')
    result = collect(args)
    log(json.dumps({key: result.get(key) for key in ('status', 'storeId', 'dateIso', 'reason', 'missingReason')}, ensure_ascii=False))
    return 0 if result['status'] in ('success', 'collected') else 2


if __name__ == '__main__':
    raise SystemExit(main())
