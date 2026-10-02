// Dsvideo source lives in its own project; releases consume this pinned snapshot.
// All npm installation happens at build time, never when the App or plugin starts.
import { createHash } from 'node:crypto'
import { spawnSync } from 'node:child_process'
import { cpSync, existsSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const plugin = join(repo, 'src-tauri/resources/plugins/dsvideo')
const inputs = join(repo, 'scripts/dsvideo-runtime')
const runtime = join(repo, 'src-tauri/resources/video-runtime')
const destination = join(runtime, 'dsvideo')
const node = join(runtime, process.platform === 'win32' ? 'node/node.exe' : 'node/bin/node')
const npm = join(runtime, process.platform === 'win32' ? 'node/node_modules/npm/bin/npm-cli.js' : 'node/lib/node_modules/npm/bin/npm-cli.js')
const argv = process.argv.slice(2)
if (argv.length && !(argv.length === 2 && argv[0] === '--source')) throw new Error('Usage: build-dsvideo-plugin.mjs [--source <dsvideo project>]')
if (!existsSync(node) || !existsSync(npm)) throw new Error('Run npm run build:video-runtime first')
const pathKey = Object.keys(process.env).find(key => key.toLowerCase() === 'path') || 'PATH'
const env = { ...process.env, [pathKey]: `${dirname(node)}${process.platform === 'win32' ? ';' : ':'}${process.env[pathKey] || ''}`, PUPPETEER_SKIP_DOWNLOAD: 'true' }
const run = (args, cwd) => {
  const result = spawnSync(node, [npm, ...args], { cwd, env, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`npm ${args[0]} exited ${result.status}`)
}
const digest = bytes => createHash('sha256').update(bytes).digest('hex')

if (argv.length) {
  const source = resolve(argv[1])
  const pkg = JSON.parse(readFileSync(join(source, 'package.json')))
  if (pkg.name !== '@dsvideo/dsvideo') throw new Error('Expected the dsvideo project')
  run(['run', 'pack:distribution'], source)
  const tarball = join(source, 'dist/release', `dsvideo-dsvideo-${pkg.version}.tgz`)
  cpSync(tarball, join(plugin, 'distribution.tgz'))
  rmSync(join(plugin, 'skills'), { recursive: true, force: true })
  cpSync(join(source, 'skills/dsvideo'), join(plugin, 'skills/dsvideo'), { recursive: true, dereference: false })
  cpSync(join(inputs, 'dsvideo-setup'), join(plugin, 'skills/dsvideo-setup'), { recursive: true })
  // Keep original references as the source snapshot. This host-specific entry defines the CLI.
  const skill = join(plugin, 'skills/dsvideo/SKILL.md')
  const original = readFileSync(skill, 'utf8')
  const heading = original.indexOf('\n# Dsvideo')
  if (heading < 0) throw new Error('Dsvideo Skill entry missing')
  const host = '\n## Dsivio 内置运行方式\n\n本插件随 Dsivio 打包，已经安装；不要安装公开 npm 包、复制其他 Provider 或读取 App 配置/密钥。\n参考文档中的 `dsvideo <参数>` 在此写作 `dsivio dsvideo <参数>`，CLI 由 App 自带 Node 运行。先用 `dsivio dsvideo --version` 与 `dsivio dsvideo doctor --endpoint dsivio.media` 检查，检查不做付费测试。\n图片、视频、配音和转写都通过 `dsivio media` 交给正在运行的 App；模型在「设置 > 媒体创作」配置，详见 references/environment/model-and-provider.md。\n转写对齐：Dsvideo 的 whisperx-alignment 调用 App 的 `dsivio media transcribe`，安装、模型与服务由 App 管理，不要另起或自行安装 WhisperX。doctor 报 DSIVIO_ASR_PREPARATION_REQUIRED 时看 `dsivio media asr status` 的 `settings` 与 `note`：开启自动安装就直接转写（首次下载约 2–5 GB，先告诉用户）；`--language` 须是设置中已勾选的语言。\n配音：`dsivio media models --kind speech` 中的 local/system-tts 使用 macOS/Windows 系统声音，无需云语音模型。按文本语言从该模型 voice.allowed 选择实际声音名，用 `dsivio media speech --model local/system-tts --text-file <UTF-8文件> --voice <名称>` 输出 WAV，或在 Dsvideo 中使用 @dsvideo/system-speech 的 system-tts。只做普通配音，不宣称声音克隆。\n预算：Dsivio 的模型可能显示 Pricing unknown，这只表示价格未公布。和用户一次确认制作范围与大致预算后，在范围内正常推进，不必每一步再问。场景/人物图、关键帧和少量试片是常规制作步骤，能让片子更好就做，不要为了少一次请求而省掉；商品视频通常先生成包含商品的场景图，再和原商品图一起作为视频参考图。明显超出约定（成片数量、时长、分辨率大幅增加或反复整片重生成）时再问。\n提交结果不确定时只查询原任务，不自动重新生成。\n项目：每次加载先运行 `dsivio dsvideo projects list`。返回的 document 所在目录的 SETUP_STATE.md 未标记 setup_completed: true 时，先读同插件 ../dsvideo-setup/SKILL.md 完成 setup；已完成则直接读取当前项目的 DSVIDEO_STATE.md，制作后更新。新建、切换或修复项目也按 setup 文档处理。\n\n'
  writeFileSync(skill, original.slice(0, heading) + '\n# Dsvideo\n' + host + original.slice(heading + '\n# Dsvideo\n'.length))
  cpSync(join(source, 'docs/public/logo-mark.svg'), join(plugin, 'logo.svg'))
  const manifest = { schemaVersion: 1, name: 'dsvideo', version: pkg.version, description: 'Dsivio 内置视频创作：编排、素材生成、Studio 预览与渲染。生成使用 App 中的媒体模型。', skills: './skills/', metadata: { icon: './logo.svg' } }
  writeFileSync(join(plugin, '.kivio-plugin/plugin.json'), JSON.stringify(manifest, null, 2) + '\n')
  writeFileSync(join(plugin, 'distribution.json'), JSON.stringify({ package: pkg.name, version: pkg.version, sha256: digest(readFileSync(tarball)) }, null, 2) + '\n')
}
const snapshot = JSON.parse(readFileSync(join(plugin, 'distribution.json')))
if (digest(readFileSync(join(plugin, 'distribution.tgz'))) !== snapshot.sha256) throw new Error('Dsvideo snapshot hash mismatch')
const platform = `${process.platform}-${process.arch}`
const fingerprint = digest(Buffer.concat([Buffer.from(platform), readFileSync(join(runtime, 'runtime.json')), readFileSync(join(plugin, 'distribution.json')), readFileSync(fileURLToPath(import.meta.url)), ...(!argv.length ? [readFileSync(join(inputs, 'package-lock.json'))] : [])]))
const marker = join(destination, 'runtime.json')
const cli = join(destination, 'node_modules/@dsvideo/dsvideo/bin/dsvideo.mjs')
if (!argv.length && existsSync(marker) && JSON.parse(readFileSync(marker)).fingerprint === fingerprint && existsSync(cli)) {
  console.log(`Bundled dsvideo ${snapshot.version} ready (${platform}).`)
  process.exit(0)
}
const stage = mkdtempSync(join(runtime, '.dsvideo-'))
try {
  cpSync(join(inputs, 'package.json'), join(stage, 'package.json'))
  if (argv.length) {
    run(['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund'], stage)
    cpSync(join(stage, 'package-lock.json'), join(inputs, 'package-lock.json'))
  } else cpSync(join(inputs, 'package-lock.json'), join(stage, 'package-lock.json'))
  run(['ci', '--omit=dev', '--no-audit', '--no-fund'], stage)
  const entry = join(stage, 'node_modules/@dsvideo/dsvideo/bin/dsvideo.mjs')
  const probe = spawnSync(node, [entry, '--version'], { encoding: 'utf8', env })
  if (probe.status !== 0 || !probe.stdout.includes(snapshot.version)) throw new Error(`Bundled dsvideo probe failed: ${probe.stderr}`)
  const finalFingerprint = digest(Buffer.concat([Buffer.from(platform), readFileSync(join(runtime, 'runtime.json')), readFileSync(join(plugin, 'distribution.json')), readFileSync(fileURLToPath(import.meta.url)), readFileSync(join(inputs, 'package-lock.json'))]))
  writeFileSync(join(stage, 'runtime.json'), JSON.stringify({ platform, version: snapshot.version, fingerprint: finalFingerprint }, null, 2) + '\n')
  const previous = `${destination}.previous`
  if (existsSync(previous)) throw new Error(`Unresolved previous dsvideo runtime: ${previous}`)
  if (existsSync(destination)) renameSync(destination, previous)
  try { renameSync(stage, destination) } catch (error) { if (existsSync(previous)) renameSync(previous, destination); throw error }
  rmSync(previous, { recursive: true, force: true })
  console.log(`Bundled dsvideo ${snapshot.version} installed (${platform}).`)
} finally { rmSync(stage, { recursive: true, force: true }) }
