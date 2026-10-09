import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
import { createInterface } from 'node:readline'
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..')
const root=resolve(process.argv[2] || join(repo,'src-tauri/resources/computer-control'))
const manifest=JSON.parse(readFileSync(join(root,'manifest.json')))
const pinned=JSON.parse(readFileSync(join(repo,'scripts/computer-control/versions.json')))
for(const name of ['cua','playwright','officecli'])assert.equal(manifest[name],pinned[name])
const exe=process.platform==='win32'?'.exe':''
const temporary=mkdtempSync(join(tmpdir(),'dsivio-control-verify-'))
const env={...process.env,HOME:temporary,USERPROFILE:temporary,PATH:process.platform==='win32'?(process.env.SystemRoot+'\\System32'):'/usr/bin:/bin',PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD:'1'}
function run(program,args) {const result=spawnSync(program,args,{env,encoding:'utf8',timeout:30000});assert.equal(result.status,0,`${program}: ${result.error || result.stderr}`);return result.stdout.trim()}
try {
 for(const [name,key] of [['playwright-cli','playwright'],['officecli','officecli']])assert.equal(run(join(root,'bin',name+exe),['--version']),pinned[key])
 // macOS CUA's signed app is installed offline by the Rust owner; do not use a
 // developer's /Applications app to claim the archive's version was verified.
 if(process.platform==='win32')assert.match(run(join(root,'bin/cua-driver.exe'),['--version']),new RegExp(pinned.cua.replaceAll('.','\\.')))
 for(const id of ['cua-driver','playwright-cli','officecli','officecli-docx','officecli-xlsx','officecli-pptx','morph-ppt'])assert.ok(existsSync(join(root,'skills',id,'SKILL.md')),`Missing ${id} skill`)
 const resources=join(temporary,'moved app with spaces','resources');const moved=join(resources,'computer-control');mkdirSync(join(moved,'bin'),{recursive:true})
 cpSync(join(root,'bin','playwright-cli'+exe),join(moved,'bin','playwright-cli'+exe));cpSync(join(root,'playwright'),join(moved,'playwright'),{recursive:true})
 const nodeRelative=process.platform==='win32'?'node/node.exe':'node/bin/node';const node=join(resources,'video-runtime',nodeRelative);mkdirSync(dirname(node),{recursive:true});cpSync(join(root,'../video-runtime',nodeRelative),node)
 assert.equal(run(join(moved,'bin','playwright-cli'+exe),['--version']),pinned.playwright)
 const child=spawn(join(root,'bin','officecli'+exe),['mcp'],{env,stdio:['pipe','pipe','pipe']})
 let stderr='';child.stderr.on('data',chunk=>stderr+=chunk)
 const lines=createInterface({input:child.stdout});const pending=new Map();let id=0
 lines.on('line',line=>{try{const message=JSON.parse(line);const waiter=pending.get(message.id);if(waiter){pending.delete(message.id);message.error?waiter.reject(new Error(JSON.stringify(message.error))):waiter.resolve(message.result)}}catch{}})
 child.on('error',error=>{for(const waiter of pending.values())waiter.reject(error)})
 child.on('exit',()=>{for(const waiter of pending.values())waiter.reject(new Error(stderr))})
 const send=message=>child.stdin.write(JSON.stringify({jsonrpc:'2.0',...message})+'\n')
 const request=(method,params={})=>new Promise((resolve,reject)=>{const requestId=++id;const timer=setTimeout(()=>{pending.delete(requestId);reject(new Error(`OfficeCLI ${method} timed out: ${stderr}`))},15000);pending.set(requestId,{resolve:value=>{clearTimeout(timer);resolve(value)},reject:error=>{clearTimeout(timer);reject(error)}});send({id:requestId,method,params})})
 try {
  const result=await request('initialize',{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'dsivio-control-check',version:'1'}});assert.ok(result.serverInfo)
  send({method:'notifications/initialized'});const tools=await request('tools/list');assert.ok(tools.tools.length>0);console.log(`OfficeCLI MCP connected: ${tools.tools.length} tools`)
 } finally {lines.close();child.stdin.end();child.kill();await new Promise(resolve=>{if(child.exitCode!==null||child.signalCode!==null)return resolve();const timer=setTimeout(()=>{child.kill('SIGKILL');resolve()},2000);child.once('exit',()=>{clearTimeout(timer);resolve()})})}
 console.log('Pinned CLI versions, bundled skills and relocated Playwright verified without system Node or package downloads.')
} finally {rmSync(temporary,{recursive:true,force:true})}
