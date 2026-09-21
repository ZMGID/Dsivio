// Launch the bundled analyzer with the user's optional local speech backend.
const { existsSync } = require('node:fs')
const { homedir } = require('node:os')
const { join } = require('node:path')
const { spawn } = require('node:child_process')

const env = { ...process.env }
if (!env.DSVIDEO_RUNTIME_ROOT) throw new Error('DSVIDEO_RUNTIME_ROOT is required')
if (!env.WHISPER_BIN) {
  const candidates = [
    join(homedir(), '.local', 'bin', process.platform === 'win32' ? 'whisper.exe' : 'whisper'),
  ]
  const installed = candidates.find(existsSync)
  if (installed) env.WHISPER_BIN = installed
}
// Multilingual small is more useful for product speech than the upstream tiny default.
env.WHISPER_MODEL ||= 'small'
// The optional Whisper executable owns its Python environment; Comfy's bundled
// PYTHONPATH must not shadow its dependencies.
delete env.PYTHONPATH
delete env.PYTHONHOME
const entry = join(env.DSVIDEO_RUNTIME_ROOT, 'analyzer', 'node_modules', 'mcp-video-analyzer', 'dist', 'index.js')
const child = spawn(process.execPath, [entry], { env, stdio: 'inherit' })
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => child.kill(signal))
child.on('error', error => { console.error(error.message); process.exit(1) })
child.on('exit', code => process.exit(code ?? 1))
