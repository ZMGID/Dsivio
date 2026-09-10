"""Controlled video planning call using existing settings; never submits media jobs.
Credentials remain in memory and are never written into evidence.
"""
import base64
import json
from pathlib import Path
import re
import urllib.request

REPO = Path(__file__).resolve().parents[3]
APP = Path.home() / 'Library/Application Support/com.zmair.kivio'
OUT = Path(__file__).resolve().parent
settings = json.loads((APP / 'settings.json').read_text())['settings']
selection = settings['defaultModels']['chat']
provider = next(p for p in settings['providers'] if p['id'] == selection['providerId'])
assert provider['apiFormat'] == 'openai_responses'
base = provider['baseUrl'].rstrip('/')
assert base == 'https://api.deepseek.com', 'Re-check adapter URL mapping before using another provider'
endpoint = base + '/responses'
key = next(k for k in provider['apiKeys'] if k.strip())
asset = APP / 'video-studio/assets/b471cc0e2406280fb486e5292149dc2c632a1230ef9709a263394de2fcab63a9.jpg'
image_url = 'data:image/jpeg;base64,' + base64.b64encode(asset.read_bytes()).decode()
agent = (REPO / 'src-tauri/src/image_studio/agent.rs').read_text()
video = (REPO / 'src-tauri/src/video_studio.rs').read_text()

def literal(source, prefix):
    match = re.search(r'"' + re.escape(prefix) + r'(?:[^"\\]|\\.)*"', source)
    assert match, prefix
    return json.loads(match[0])

video_system = literal(agent, "You are Dsivio's video director.")
guide = (REPO / 'src-tauri/resources/plugins/dsvideo-plugin/skills/video-director/SKILL.md').read_text()
grok_note = None
# The Rust literal contains an escaped newline, decoded by literal().
for raw in re.findall(r'"(?:[^"\\]|\\.)*"', video):
    decoded = json.loads(raw)
    if decoded.startswith('{guide}\nGrok'): grok_note = decoded.replace('{guide}', guide)
    if decoded.startswith('{guide}\nFor a broad request'): plan_instruction = decoded
assert grok_note
plan_instruction = plan_instruction.replace('{guide}', grok_note).replace('{{', '{').replace('}}', '}')
video_system = video_system.replace('{instruction}', plan_instruction)
brief = {'mode': 'creation', 'request': '15秒，9:16，固定机位展示参考图中的小夜灯由未点亮到单一暖白光亮起。只展示商品，不出现人物、手或字幕，不增加道具；图案全程保持与参考图一致。', 'selectedConcept': '固定机位拍摄商品从未点亮到暖白光亮起', 'images': [str(asset)], 'duration': 15, 'ratio': '9:16', 'route': 'grok', 'resolution': '720p', 'inputMode': 'auto', 'speechMode': 'ambient', 'language': 'zh-CN'}

def check(name, system, data):
    payload = {'model': selection['model'], 'stream': False, 'instructions': system, 'input': [
        {'role': 'user', 'content': [{'type': 'input_text', 'text': json.dumps(data, ensure_ascii=False)}, {'type': 'input_text', 'text': '商品参考图'}, {'type': 'input_image', 'image_url': image_url}]}]}
    request = urllib.request.Request(endpoint, data=json.dumps(payload).encode(), headers={'Content-Type': 'application/json', 'Authorization': 'Bearer ' + key})
    with urllib.request.urlopen(request, timeout=180) as response:
        result = json.load(response)
    text = '\n'.join(c.get('text', '') for item in result.get('output', []) for c in item.get('content', []) if c.get('type') == 'output_text')
    evidence = {'model': selection['model'], 'endpoint': endpoint, 'scope': 'Planning-only API check; no generated image/video and no task mutation', 'request': data, 'output': text, 'usage': result.get('usage')}
    (OUT / (name + '.json')).write_text(json.dumps(evidence, ensure_ascii=False, indent=2))
    print(name, text, flush=True)

if __name__ == '__main__':
    check('video-plan', video_system, brief)
