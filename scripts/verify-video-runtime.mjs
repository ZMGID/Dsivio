// Exercise shipped executables without developer PATH or package caches.
import { spawn, spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, renameSync, rmSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createInterface } from 'node:readline'
import assert from 'node:assert/strict'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const original = resolve(process.argv[2] || join(repo, 'src-tauri/resources/video-runtime'))
const relocate = process.argv.includes('--relocate')
const root = relocate ? `${original} relocated with spaces` : original
if (relocate) renameSync(original, root)
const scratch = mkdtempSync(join(tmpdir(), 'dsivio-video-smoke-'))
const win = process.platform === 'win32'
const python = join(root, win ? 'python/python.exe' : 'python/bin/python3')
const node = join(root, win ? 'node/node.exe' : 'node/bin/node')
const npm = join(root, win ? 'node/node_modules/npm/bin/npm-cli.js' : 'node/lib/node_modules/npm/bin/npm-cli.js')
const ffmpeg = join(root, `analyzer/node_modules/ffmpeg-static/ffmpeg${win ? '.exe' : ''}`)
const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !['path', 'pythonpath', 'pythonhome', 'node_options', 'node_path'].includes(k.toLowerCase())))
Object.assign(env, { PATH: [join(root, 'bin'), dirname(python), dirname(node), dirname(ffmpeg)].join(win ? ';' : ':'),
  DSVIDEO_RUNTIME_ROOT: root, PYTHONPATH: join(root, 'python-packages'), PYTHONNOUSERSITE: '1',
  PYTHONUTF8: '1', PYTHONIOENCODING: 'utf-8',
  GIT_PYTHON_REFRESH: 'quiet', DSVIDEO_CONFIG_PATH: join(scratch, 'providers.json'), DSVIDEO_STUDIO_ROOT: join(scratch, 'studio'),
  npm_config_cache: join(scratch, 'empty-npm-cache'), UV_OFFLINE: '1', PIP_NO_INDEX: '1' })
Object.assign(env, {
  HOME: join(scratch, 'clean-home'), USERPROFILE: join(scratch, 'clean-home'),
  // npm defaults to a bare `sh` on Unix; the smoke PATH intentionally has no OS
  // tools. Select the OS shell explicitly while keeping Node resolution bundled.
  npm_config_script_shell: win ? process.env.ComSpec || join(process.env.SystemRoot || 'C:\\Windows', 'System32/cmd.exe') : '/bin/sh',
})
mkdirSync(env.HOME)

function run(command, args, input) {
  const result = spawnSync(command, args, { env, cwd: scratch, encoding: 'utf8', timeout: 45000, input })
  assert.equal(result.status, 0, `${command}: ${result.error || result.stderr || result.stdout}`)
  return result.stdout
}
async function withMcp(command, args, check) {
  const child = spawn(command, args, { env, cwd: scratch, stdio: ['pipe', 'pipe', 'pipe'] })
  let stderr = '', id = 0
  const pending = new Map()
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-6000) })
  child.on('error', error => { for (const value of pending.values()) value.reject(error) })
  child.on('exit', code => { for (const value of pending.values()) value.reject(new Error(`MCP exited ${code}: ${stderr}`)) })
  const lines = createInterface({ input: child.stdout })
  lines.on('line', line => {
    let response
    try { response = JSON.parse(line) } catch { return }
    const value = pending.get(response.id)
    if (!value) return
    pending.delete(response.id)
    if (response.error) value.reject(new Error(JSON.stringify(response.error)))
    else value.resolve(response.result)
  })
  function send(message) { child.stdin.write(JSON.stringify({ jsonrpc: '2.0', ...message }) + '\n') }
  async function request(method, params = {}) {
    const requestId = ++id
    let timer
    try {
      return await new Promise((resolve, reject) => {
        timer = setTimeout(() => { pending.delete(requestId); reject(new Error(`${method} timed out: ${stderr}`)) }, 45000)
        pending.set(requestId, { resolve, reject })
        send({ id: requestId, method, params })
      })
    } finally { clearTimeout(timer) }
  }
  try {
    await request('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'dsivio-bundle-check', version: '1' } })
    send({ method: 'notifications/initialized' })
    await check(request)
  } finally {
    lines.close()
    if (child.pid !== undefined) {
      child.stdin.end()
      child.kill()
      await new Promise(resolve => {
        if (child.exitCode !== null || child.signalCode !== null) { resolve(); return }
        const deadline = setTimeout(() => { child.kill('SIGKILL'); resolve() }, 2000)
        child.once('exit', () => { clearTimeout(deadline); resolve() })
      })
    }
  }
}
try {
  assert.equal(JSON.parse(readFileSync(join(root, 'runtime.json'))).platform, `${process.platform}-${process.arch}`)
  assert.equal(run(node, [npm, '--version']).trim(), JSON.parse(readFileSync(join(root, 'runtime.json'))).npm)
  if (!process.argv.includes('--base-runtime-only')) {
    const dsvideo = join(root, 'dsvideo/node_modules/@dsvideo/dsvideo/bin/dsvideo.mjs')
    const dsvideoVersion = JSON.parse(readFileSync(join(root, 'dsvideo/runtime.json'))).version
    assert.equal(run(node, [dsvideo, '--version']).trim(), dsvideoVersion)
    run(node, [dsvideo, 'runtime', 'init', '--workspace', scratch])
    assert.ok(existsSync(join(scratch, '.dsvideo/runtime')), 'bundled dsvideo must initialize a local workspace')
  }
  // Prove explicit setup can install a local dependency (including its native-style
  // postinstall), with an empty HOME/cache and no system Node/npm.
  const dependency = join(scratch, 'local-dependency')
  mkdirSync(dependency)
  writeFileSync(join(dependency, 'package.json'), JSON.stringify({
    name: 'dsivio-runtime-smoke-dependency', version: '1.0.0', main: 'index.cjs',
    scripts: { postinstall: 'node postinstall.cjs' },
  }))
  writeFileSync(join(dependency, 'postinstall.cjs'), 'require("node:fs").writeFileSync("node-path.txt", process.execPath)')
  writeFileSync(join(dependency, 'index.cjs'), 'module.exports = require("node:fs").readFileSync(require("node:path").join(__dirname, "node-path.txt"), "utf8")')
  writeFileSync(join(scratch, 'package.json'), JSON.stringify({
    name: 'dsivio-runtime-smoke', version: '1.0.0', private: true,
    dependencies: { 'dsivio-runtime-smoke-dependency': 'file:./local-dependency' },
  }))
  run(node, [npm, 'install', '--omit=dev', '--no-audit', '--no-fund', '--offline'])
  assert.equal(run(node, ['-p', 'require("dsivio-runtime-smoke-dependency")']).trim(), node)
  const bundledScripts = join(dirname(root), 'video-studio/scripts')
  const scripts = existsSync(bundledScripts) ? bundledScripts : join(repo, 'src-tauri/resources/video-studio/scripts')
  // openpyxl/Pillow/lxml back the bundled report Skills (`dsivio python`): import their native parts.
  const directImports = `import comfy_mcp, comfy_cli, yt_dlp, openpyxl, PIL.Image, lxml.etree${win ? ', pywintypes' : ''}`
  const directPython = spawnSync(python, ['-s', '-B', '-c', directImports], { cwd: scratch, env: { ...env, PYTHONPATH: '' }, encoding: 'utf8', timeout: 45000 })
  assert.equal(directPython.status, 0, `bundled Python must work without PYTHONPATH: ${directPython.stderr}`)
  run(join(root, 'bin', win ? 'comfy.exe' : 'comfy'), ['--help'])
  run(join(root, 'bin', win ? 'yt-dlp.exe' : 'yt-dlp'), ['--version'])
  const video = join(scratch, 'local clip.mp4')
  run(ffmpeg, ['-y', '-f', 'lavfi', '-i', 'color=c=blue:s=64x64:d=1', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', video])
  const probe = JSON.parse(run(join(root, 'bin', win ? 'ffprobe.exe' : 'ffprobe'), ['-v', 'error', '-show_entries', 'format=duration', '-of', 'json', video]))
  assert.ok(Number(probe.format.duration) > 0, 'bundled ffprobe must read local media')
  await withMcp(python, ['-s', '-B', join(scripts, 'mcp_comfy.py')], async request => {
    const result = await request('tools/list')
    assert.ok(result.tools.some(t => t.name === 'run_workflow'), 'Comfy tools unavailable')
    console.log(`Bundled Comfy MCP: ${result.tools.length} tools`)
  })
  await withMcp(node, [join(root, 'analyzer/node_modules/mcp-video-analyzer/dist/index.js')], async request => {
    const result = await request('tools/list')
    assert.ok(result.tools.some(t => t.name === 'get_metadata'))
    const metadata = await request('tools/call', { name: 'get_metadata', arguments: { url: video } })
    assert.ok(!metadata.isError, JSON.stringify(metadata))
    const frame = await request('tools/call', { name: 'get_frame_at', arguments: { url: video, timestamp: '0:00' } })
    assert.ok(!frame.isError && frame.content.some(item => item.type === 'image'), 'bundled analyzer must decode a local video frame')
    console.log(`Bundled video analyzer: ${result.tools.length} tools; local metadata and frame extraction succeeded`)
  })
  console.log('Bundled runtime verified without system Node/Python/npm/ffmpeg; bundled npm installed and loaded a local dependency in a clean HOME.')
} finally {
  rmSync(scratch, { recursive: true, force: true })
  if (relocate) renameSync(root, original)
}
