"""Exact reported brief, planning only. No video submissions or task changes."""
import base64,json,re,urllib.request
from pathlib import Path
REPO=Path(__file__).resolve().parents[3]
APP=Path.home()/'Library/Application Support/com.zmair.kivio'
OUT=Path(__file__).resolve().parent

def literals(file):
    return [json.loads(s) for s in re.findall(r'"(?:[^"\\]|\\.)*"',file.read_text())]

settings=json.loads((APP/'settings.json').read_text())['settings']
default=settings['defaultModels']['chat']
assistant=next(a for a in json.loads((APP/'conversations/assistants.json').read_text())['assistants'] if a['id']=='asst_builtin_video_ugc')
provider=next(p for p in settings['providers'] if p['id']==(assistant.get('provider_id') or default['providerId']))
model=assistant.get('model') or default['model']
assert provider['apiFormat']=='openai_responses' and provider['baseUrl'].rstrip('/')=='https://api.deepseek.com'
key=next(k for k in provider['apiKeys'] if k.strip())
source=(REPO/'src-tauri/src/chat/storage/video_assistants.rs').read_text()
prompt=re.search(r'const UGC: &str = r#"(.*?)"#;',source,re.S)[1]
parts=literals(REPO/'src-tauri/src/video_studio/planning.rs')
protocol=next(s for s in parts if s.startswith('{}\n\n工作台输出协议：'))
output=next(s for s in parts if s.startswith('返回 {'))
service=next(s for s in parts if s.startswith('为兼容已知 Grok'))
instruction=protocol.replace('{}',prompt,1).replace('{output}',output).replace('{service}',service)
system=next(s for s in literals(REPO/'src-tauri/src/image_studio/agent.rs') if s.startswith("You are Dsivio's video director.")).replace('{instruction}',instruction)
task=json.loads((APP/'video-studio/tasks/4638fd15-de41-43c8-b348-b99978ee71ed.json').read_text())
assert task['prompt']==task['script']
brief=task['brief']
asset=Path(brief['images'][0]); image='data:image/jpeg;base64,'+base64.b64encode(asset.read_bytes()).decode()
cases=[('reported-auto',{}),('ambient',{'speechMode':'ambient'}),('explicit-no-speech',{'request':'tk店铺展示视频，不要人物，不要口播'})]
import sys
if len(sys.argv)>1: cases=[case for case in cases if case[0] in sys.argv[1:]]
for name,patch in cases:
    data={**brief,**patch}
    payload={'model':model,'stream':False,'instructions':system,'input':[{'role':'user','content':[{'type':'input_text','text':json.dumps(data,ensure_ascii=False)},{'type':'input_text','text':'参考图片 1（用途以用户要求为准，不默认用作首帧）'},{'type':'input_image','image_url':image}]}]}
    req=urllib.request.Request(provider['baseUrl'].rstrip('/')+'/responses',data=json.dumps(payload).encode(),headers={'Content-Type':'application/json','Authorization':'Bearer '+key})
    with urllib.request.urlopen(req,timeout=180) as r: response=json.load(r)
    text='\n'.join(c.get('text','') for o in response.get('output',[]) for c in o.get('content',[]) if c.get('type')=='output_text')
    result=json.loads(text.strip().removeprefix('```json').removesuffix('```').strip())
    assert result.get('script'),result
    (OUT/(name+'.json')).write_text(json.dumps({'scope':'Reconstructed host planning-only request. No media jobs.','model':model,'brief':data,'instructions':system,'output':result,'usage':response.get('usage')},ensure_ascii=False,indent=2))
    print(name,result['script'],flush=True)
