"""Three controlled planning-only calls. Never submit video jobs or write credentials.
Run after the app's v10 assistant migration. Instructions are read from the current
Rust literals plus the actual saved assistant prompts, not duplicated templates.
"""
import base64
import json
import re
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
APP = Path.home() / 'Library/Application Support/com.zmair.kivio'
OUT = Path(__file__).resolve().parent

def strings(path):
    return [json.loads(raw) for raw in re.findall(r'"(?:[^"\\]|\\.)*"', path.read_text())]

def main():
    settings = json.loads((APP / 'settings.json').read_text())['settings']
    assistants = json.loads((APP / 'conversations/assistants.json').read_text())['assistants']
    instructions = strings(REPO / 'src-tauri/src/video_studio/planning.rs')
    output = next(s for s in instructions if s.startswith('返回 {'))
    service = next(s for s in instructions if s.startswith('为兼容已知 Grok'))
    protocol = next(s for s in instructions if s.startswith('{}\n\n工作台输出协议：'))
    system = next(s for s in strings(REPO / 'src-tauri/src/image_studio/agent.rs') if s.startswith("You are Dsivio's video director."))
    asset = APP / 'video-studio/assets/b471cc0e2406280fb486e5292149dc2c632a1230ef9709a263394de2fcab63a9.jpg'
    image = 'data:image/jpeg;base64,' + base64.b64encode(asset.read_bytes()).decode()
    cases = [
        ('asst_builtin_video_prompt', '固定机位展示参考图中的小夜灯，保持现有亮度。只展示商品，不出现人物、手或字幕，不增加道具，图案以参考图为准。', 'ambient'),
        ('asst_builtin_video_product', '做商品展示视频：镜头从小夜灯整体缓慢推近，展示发光图案。不要人物、手、操作演示、字幕或新道具。外观以参考图为准，不猜测图案含义。', 'ambient'),
        ('asst_builtin_video_ugc', '一位成年人在卧室展示参考图中的小夜灯，普通话说：“这盏小夜灯，放在床头刚刚好。” 不要字幕，不增加其他人物，不编造功能或使用效果。', 'dialogue'),
    ]
    for assistant_id, request, speech in cases:
        assistant = next(a for a in assistants if a['id'] == assistant_id)
        assert assistant['category'] == 'video'
        default = settings['defaultModels']['chat']
        provider_id = assistant.get('provider_id') or default['providerId']
        model = assistant.get('model') or default['model']
        provider = next(p for p in settings['providers'] if p['id'] == provider_id)
        assert provider['apiFormat'] == 'openai_responses'
        assert provider['baseUrl'].rstrip('/') == 'https://api.deepseek.com', 'Verify adapter before another endpoint'
        endpoint = provider['baseUrl'].rstrip('/') + '/responses'
        key = next(k for k in provider['apiKeys'] if k.strip())
        instruction = protocol.replace('{}', assistant['system_prompt'], 1).replace('{output}', output).replace('{service}', service)
        full_system = system.replace('{instruction}', instruction)
        brief = {'assistantId': assistant_id, 'mode': 'creation', 'request': request, 'images': [str(asset)], 'duration': 8, 'ratio': '9:16', 'route': 'grok', 'resolution': '720p', 'inputMode': 'auto', 'speechMode': speech, 'language': 'zh-CN'}
        if speech == 'dialogue': brief['dialogue'] = '这盏小夜灯，放在床头刚刚好。'
        payload = {'model': model, 'stream': False, 'instructions': full_system, 'input': [{'role': 'user', 'content': [
            {'type': 'input_text', 'text': json.dumps(brief, ensure_ascii=False)},
            {'type': 'input_text', 'text': '参考图片 1（用途以用户要求为准，不默认用作首帧）'},
            {'type': 'input_image', 'image_url': image},
        ]}]}
        req = urllib.request.Request(endpoint, data=json.dumps(payload).encode(), headers={'Content-Type': 'application/json', 'Authorization': 'Bearer ' + key})
        with urllib.request.urlopen(req, timeout=180) as response:
            result = json.load(response)
        text = '\n'.join(c.get('text', '') for item in result.get('output', []) for c in item.get('content', []) if c.get('type') == 'output_text')
        parsed = json.loads(text.strip().removeprefix('```json').removesuffix('```').strip())
        assert parsed.get('script'), 'Specific requests should return a script'
        record = {'assistant': assistant['name'], 'assistantId': assistant_id, 'model': model, 'endpoint': endpoint, 'scope': 'Planning-only; no video generation, no task mutation. Reconstructed current host instruction and media payload, not end-to-end Tauri invocation.', 'brief': brief, 'instructions': full_system, 'output': parsed, 'usage': result.get('usage')}
        (OUT / (assistant_id + '.json')).write_text(json.dumps(record, ensure_ascii=False, indent=2))
        print(assistant['name'], parsed['script'], flush=True)

if __name__ == '__main__':
    main()
