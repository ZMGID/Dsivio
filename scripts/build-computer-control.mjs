// Build-time downloads only. First launch installs these verified local resources offline.
import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const inputs = join(repo, 'scripts/computer-control')
const versions = JSON.parse(readFileSync(join(inputs, 'versions.json')))
const platform = `${process.platform}-${process.arch}`
const assets = versions.platforms[platform]
if (!assets) throw new Error(`Unsupported computer-control platform: ${platform}`)
const destination = join(repo, 'src-tauri/resources/computer-control')
const fingerprint = createHash('sha256')
for (const file of ['versions.json', 'package.json', 'package-lock.json', 'playwright-launcher.rs']) fingerprint.update(readFileSync(join(inputs, file)))
fingerprint.update(readFileSync(fileURLToPath(import.meta.url))).update(platform)
const identity = fingerprint.digest('hex')
const marker = join(destination, 'manifest.json')
const exe = process.platform === 'win32' ? '.exe' : ''
const required = [`bin/officecli${exe}`, `bin/cua-driver${exe}`, `bin/playwright-cli${exe}`, 'skills/cua-driver/SKILL.md', 'skills/playwright-cli/SKILL.md', 'skills/officecli/SKILL.md', 'playwright/node_modules/@playwright/cli/playwright-cli.js']
if (process.platform === 'darwin') required.push('CuaDriver.zip')
if (existsSync(marker) && JSON.parse(readFileSync(marker)).fingerprint === identity && required.every(file => existsSync(join(destination,file)))) { console.log('Pinned computer-control bundle ready.'); process.exit(0) }
function run(program,args,options={}) { const r=spawnSync(program,args,{stdio:'inherit',...options}); if(r.error) throw r.error; if(r.status !== 0) throw new Error(`${program} exited ${r.status}`) }
const cache=join(tmpdir(),'dsivio-control-downloads'); mkdirSync(cache,{recursive:true})
async function download(asset,url) {
 const path=join(cache,asset.file)
 let data=existsSync(path)?readFileSync(path):null
 if(!data || createHash('sha256').update(data).digest('hex') !== asset.sha256) { const r=await fetch(url,{signal:AbortSignal.timeout(180000)}); if(!r.ok) throw new Error(`${url}: HTTP ${r.status}`); data=Buffer.from(await r.arrayBuffer()) }
 if(createHash('sha256').update(data).digest('hex') !== asset.sha256) throw new Error(`Checksum mismatch: ${asset.file}`)
 writeFileSync(path,data); return path
}
mkdirSync(dirname(destination),{recursive:true})
const stage=mkdtempSync(join(dirname(destination),'.computer-control-'))
try {
 const bin=join(stage,'bin'); const skills=join(stage,'skills'); mkdirSync(bin); mkdirSync(skills)
 const cuaBase=`https://github.com/trycua/cua/releases/download/cua-driver-rs-v${versions.cua}/`
 const cua=await download(assets.cua,cuaBase+assets.cua.file)
 const unpack=join(stage,'cua-unpack'); mkdirSync(unpack); run('tar',['-xf',cua,'-C',unpack])
 function find(root,name) { for(const e of readdirSync(root,{withFileTypes:true})) { const p=join(root,e.name); if(e.name===name) return p; if(e.isDirectory()) {const hit=find(p,name); if(hit)return hit} } }
 if(process.platform==='darwin') {
  const app=find(unpack,'CuaDriver.app'); if(!app)throw new Error('Signed CUA app missing')
  run('codesign',['--verify','--deep','--strict',app])
  run('ditto',['-c','-k','--sequesterRsrc','--keepParent',app,join(stage,'CuaDriver.zip')])
  writeFileSync(join(bin,'cua-driver'),'#!/bin/sh\nexec /Applications/CuaDriver.app/Contents/MacOS/cua-driver "$@"\n'); chmodSync(join(bin,'cua-driver'),0o755)
 } else { const driver=find(unpack,'cua-driver.exe'); if(!driver)throw new Error('CUA executable missing'); cpSync(dirname(driver),bin,{recursive:true}) }
 for(const name of ['LICENSE','THIRD_PARTY_NOTICES.md']) {const file=find(unpack,name);if(!file)throw new Error(`CUA notice missing: ${name}`);cpSync(file,join(stage,`CUA-${name}`))}
 rmSync(unpack,{recursive:true,force:true})
 const cuaSkills=await download(versions.cuaSkills,cuaBase+versions.cuaSkills.file)
 const skillUnpack=join(stage,'skill-unpack'); mkdirSync(skillUnpack); run('tar',['-xf',cuaSkills,'-C',skillUnpack])
 const cuaSkill=join(skillUnpack,`cua-driver-rs-v${versions.cua}-skills`); if(!existsSync(join(cuaSkill,'SKILL.md')))throw new Error('CUA skill pack missing')
 cpSync(cuaSkill,join(skills,'cua-driver'),{recursive:true}); rmSync(skillUnpack,{recursive:true,force:true})
 const office=await download(assets.office,`https://github.com/iOfficeAI/OfficeCLI/releases/download/v${versions.officecli}/${assets.office.file}`)
 cpSync(office,join(bin,`officecli${exe}`)); chmodSync(join(bin,`officecli${exe}`),0o755)
 const source=await download(versions.officeSource,`https://codeload.github.com/iOfficeAI/OfficeCLI/tar.gz/refs/tags/v${versions.officecli}`)
 const officeStage=join(stage,'office-source');mkdirSync(officeStage);run('tar',['-xf',source,'-C',officeStage])
 const officeRoot=join(officeStage,readdirSync(officeStage)[0]);cpSync(join(officeRoot,'skills'),skills,{recursive:true});cpSync(join(officeRoot,'LICENSE'),join(stage,'OfficeCLI-LICENSE'));rmSync(officeStage,{recursive:true,force:true})
 const playwright=join(stage,'playwright');mkdirSync(playwright)
 for(const file of ['package.json','package-lock.json'])cpSync(join(inputs,file),join(playwright,file))
 const runtime=join(repo,'src-tauri/resources/video-runtime');const node=join(runtime,process.platform==='win32'?'node/node.exe':'node/bin/node');const npm=join(runtime,process.platform==='win32'?'node/node_modules/npm/bin/npm-cli.js':'node/lib/node_modules/npm/bin/npm-cli.js')
 const key=Object.keys(process.env).find(k=>k.toLowerCase()==='path')||'PATH'
 run(node,[npm,'ci','--omit=dev','--no-audit','--no-fund'],{cwd:playwright,env:{...process.env,PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD:'1',[key]:`${dirname(node)}${process.platform==='win32'?';':':'}${process.env[key]||''}`}})
 run('rustc',['--edition=2021','-C','opt-level=s','-C','strip=symbols',join(inputs,'playwright-launcher.rs'),'-o',join(bin,`playwright-cli${exe}`)])
 const pwSkill=join(playwright,'node_modules/@playwright/cli/skills/playwright-cli')
 if(!existsSync(join(pwSkill,'SKILL.md')))throw new Error('Pinned Playwright skill pack missing')
 cpSync(pwSkill,join(skills,'playwright-cli'),{recursive:true})
 writeFileSync(join(stage,'manifest.json'),JSON.stringify({...versions,platform,fingerprint:identity},null,2)+'\n')
 for(const file of required)if(!existsSync(join(stage,file)))throw new Error(`Missing bundled control resource: ${file}`)
 run(join(bin,`officecli${exe}`),['--version'])
 run(join(bin,`playwright-cli${exe}`),['--version'])
 rmSync(destination,{recursive:true,force:true});renameSync(stage,destination);console.log(`Pinned CUA ${versions.cua}, Playwright ${versions.playwright}, OfficeCLI ${versions.officecli} bundled.`)
} finally {rmSync(stage,{recursive:true,force:true})}
