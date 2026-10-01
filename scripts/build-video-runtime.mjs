// Build-time only: download pinned runtimes and install locked dependencies.
// Market setup can use bundled npm explicitly; application startup never installs dependencies.
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const asrCheck = spawnSync(process.execPath, [join(repo, 'scripts/sync-dsivio-video-asr.mjs'), '--check'], { stdio: 'inherit' })
if (asrCheck.error) throw asrCheck.error
if (asrCheck.status !== 0) throw new Error('Bundled ASR snapshot verification failed')
const inputs = join(repo, 'scripts/video-runtime')
const destination = join(repo, 'src-tauri/resources/video-runtime')
const versions = JSON.parse(readFileSync(join(inputs, 'versions.json')))
const platform = `${process.platform}-${process.arch}`
const archive = versions.nodeArchives[platform]
if (!archive) throw new Error(`Bundled video runtime does not support ${platform}; build on the target OS/architecture.`)
const fingerprint = createHash('sha256').update(platform)
for (const file of ['versions.json', 'package.json', 'package-lock.json', 'requirements.txt', 'launcher.rs']) {
  fingerprint.update(readFileSync(join(inputs, file)))
}
fingerprint.update(readFileSync(fileURLToPath(import.meta.url)))
const identity = fingerprint.digest('hex')
const marker = join(destination, 'runtime.json')
const required = ['python-packages/comfy_mcp/server.py', 'analyzer/node_modules/mcp-video-analyzer/dist/index.js',
  process.platform === 'win32' ? 'python/python.exe' : 'python/bin/python3',
  process.platform === 'win32' ? 'node/node.exe' : 'node/bin/node',
  process.platform === 'win32' ? 'node/node_modules/npm/bin/npm-cli.js' : 'node/lib/node_modules/npm/bin/npm-cli.js',
  `bin/comfy${process.platform === 'win32' ? '.exe' : ''}`, `bin/yt-dlp${process.platform === 'win32' ? '.exe' : ''}`,
  `bin/ffprobe${process.platform === 'win32' ? '.exe' : ''}`,
  `analyzer/node_modules/ffmpeg-static/ffmpeg${process.platform === 'win32' ? '.exe' : ''}`]
if (existsSync(marker) && JSON.parse(readFileSync(marker)).fingerprint === identity && required.every(p => existsSync(join(destination, p)))) {
  console.log(`Bundled video runtime ready (${platform}).`)
  process.exit(0)
}
function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: 'inherit', ...options })
  if (result.error) throw result.error
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}`)
}
mkdirSync(dirname(destination), { recursive: true })
const staging = mkdtempSync(join(dirname(destination), '.video-runtime-'))
try {
  console.log(`Preparing bundled video runtime for ${platform}…`)
  const cache = join(tmpdir(), 'dsivio-runtime-downloads')
  mkdirSync(cache, { recursive: true })
  const cached = join(cache, archive.file)
  let bytes = existsSync(cached) ? readFileSync(cached) : null
  if (!bytes || createHash('sha256').update(bytes).digest('hex') !== archive.sha256) {
    const download = await fetch(`https://nodejs.org/dist/v${versions.node}/${archive.file}`, { signal: AbortSignal.timeout(120000) })
    if (!download.ok) throw new Error(`Node download: HTTP ${download.status}`)
    bytes = Buffer.from(await download.arrayBuffer())
  }
  if (createHash('sha256').update(bytes).digest('hex') !== archive.sha256) throw new Error('Node archive checksum mismatch')
  writeFileSync(cached, bytes)
  const tarball = join(staging, archive.file)
  writeFileSync(tarball, bytes)
  run('tar', ['-xf', tarball, '-C', staging])
  rmSync(tarball)
  const nodeDirectory = readdirSync(staging).find(p => p.startsWith('node-v'))
  renameSync(join(staging, nodeDirectory), join(staging, 'node'))
  const node = join(staging, process.platform === 'win32' ? 'node/node.exe' : 'node/bin/node')
  const npm = join(staging, process.platform === 'win32' ? 'node/node_modules/npm/bin/npm-cli.js' : 'node/lib/node_modules/npm/bin/npm-cli.js')
  const analyzer = join(staging, 'analyzer')
  mkdirSync(analyzer)
  for (const file of ['package.json', 'package-lock.json']) cpSync(join(inputs, file), join(analyzer, file))
  // Use the bundled Node for dependency postinstall scripts as well (sharp, ffmpeg).
  const pathKey = Object.keys(process.env).find(k => k.toLowerCase() === 'path') || 'PATH'
  const buildEnv = { ...process.env, [pathKey]: `${dirname(node)}${process.platform === 'win32' ? ';' : ':'}${process.env[pathKey] || ''}` }
  run(node, [npm, 'ci', '--omit=dev', '--no-audit', '--no-fund'], { cwd: analyzer, env: buildEnv })

  const pythonInstall = join(staging, 'python-install')
  run('uv', ['python', 'install', versions.python, '--install-dir', pythonInstall, '--no-bin', '--no-registry'])
  const pythonDirectory = readdirSync(pythonInstall).find(p => p.startsWith(`cpython-${versions.python}-`))
  if (!pythonDirectory) throw new Error('Standalone Python installation missing')
  renameSync(join(pythonInstall, pythonDirectory), join(staging, 'python'))
  rmSync(pythonInstall, { recursive: true, force: true })
  const python = join(staging, process.platform === 'win32' ? 'python/python.exe' : 'python/bin/python3')
  run('uv', ['pip', 'install', '--python', python, '--target', join(staging, 'python-packages'),
    '--require-hashes', '--only-binary', ':all:', '-r', join(inputs, 'requirements.txt')])
  // Resolve shipped packages relative to this interpreter, including when chat
  // skills invoke Python directly without the MCP-specific environment.
  const site = join(staging, process.platform === 'win32' ? 'python/Lib/site-packages' : `python/lib/python${versions.python.split('.').slice(0, 2).join('.')}/site-packages`)
  mkdirSync(site, { recursive: true })
  writeFileSync(join(site, 'dsivio-runtime.pth'), process.platform === 'win32'
    ? '../../../python-packages\n../../../python-packages/win32\n../../../python-packages/win32/lib\n../../../python-packages/pythonwin\nimport pywin32_bootstrap\n'
    : '../../../../python-packages\n')
  mkdirSync(join(staging, 'bin'))
  const probe = spawnSync(node, ['-p', 'require("@ffprobe-installer/ffprobe").path'], { cwd: analyzer, encoding: 'utf8' })
  if (probe.status !== 0) throw new Error(`Cannot locate bundled ffprobe: ${probe.stderr}`)
  cpSync(probe.stdout.trim(), join(staging, 'bin', process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe'))
  // The installer selects a native binary, including macOS arm64. Once copied,
  // its platform packages are no longer needed in the shipped analyzer.
  rmSync(join(analyzer, 'node_modules/@ffprobe-installer'), { recursive: true, force: true })
  const launcher = join(staging, 'bin', process.platform === 'win32' ? 'comfy.exe' : 'comfy')
  run('rustc', ['--edition=2021', '-C', 'opt-level=s', '-C', 'strip=symbols', join(inputs, 'launcher.rs'), '-o', launcher])
  cpSync(launcher, join(staging, 'bin', process.platform === 'win32' ? 'yt-dlp.exe' : 'yt-dlp'))

  // Keep npm's JS entry and dependencies for explicit project setup. Remove
  // unrelated managers and shell launchers; npm always runs through bundled Node.
  rmSync(join(staging, 'python-packages/bin'), { recursive: true, force: true })
  rmSync(join(staging, 'python-packages/Scripts'), { recursive: true, force: true })
  const nodeModules = join(staging, process.platform === 'win32' ? 'node/node_modules' : 'node/lib/node_modules')
  for (const directory of readdirSync(nodeModules)) {
    if (directory !== 'npm') rmSync(join(nodeModules, directory), { recursive: true, force: true })
  }
  for (const file of ['npm', 'npx', 'corepack', 'npm.cmd', 'npx.cmd', 'corepack.cmd', 'npm.ps1', 'npx.ps1', 'corepack.ps1']) {
    rmSync(join(dirname(node), file), { force: true })
  }
  const npmVersion = JSON.parse(readFileSync(join(nodeModules, 'npm/package.json'))).version
  writeFileSync(join(staging, 'runtime.json'), JSON.stringify({ ...versions, npm: npmVersion, platform, fingerprint: identity }, null, 2) + '\n')
  writeFileSync(join(staging, '.gitkeep'), '')
  // Fail before replacing an older bundle if relocation, explicit npm setup or
  // either MCP handshake is broken. MCP startup never installs dependencies.
  run(process.execPath, [join(repo, 'scripts/verify-video-runtime.mjs'), staging, '--relocate'])
  rmSync(destination, { recursive: true, force: true })
  renameSync(staging, destination)
  console.log(`Bundled video runtime written to ${destination}`)
} finally {
  rmSync(staging, { recursive: true, force: true })
}
