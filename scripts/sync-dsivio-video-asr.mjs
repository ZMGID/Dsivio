#!/usr/bin/env node
// Release-time snapshot only. Python remains owned by dsivio-video/services/asr.
import { createHash } from 'node:crypto'
import { readFileSync, mkdirSync, writeFileSync, renameSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const args = process.argv.slice(2)
function argument(name) {
  const index = args.indexOf(name)
  if (index < 0 || !args[index + 1] || args[index + 1].startsWith('--')) throw new Error(`Required ${name} <value>`)
  return args[index + 1]
}
const check = args.includes('--check')
if (check && args.length === 1) {
  const destination = path.join(root, 'src-tauri/resources/dsivio-video-asr/0.2.0')
  const manifest = JSON.parse(readFileSync(path.join(destination, 'manifest.json'), 'utf8'))
  if (manifest.serviceVersion !== '0.2.0' || manifest.protocol !== 'dsivio-video.asr/1' || manifest.whisperxVersion !== '3.8.6' || Object.keys(manifest.files).length !== 3) throw new Error('Invalid ASR release manifest')
  for (const name of ['server.py', 'prepare.py', 'requirements.txt']) {
    if (createHash('sha256').update(readFileSync(path.join(destination, name))).digest('hex') !== manifest.files[name]) throw new Error(`ASR resource hash mismatch: ${name}`)
  }
  console.log(`Verified ASR ${manifest.serviceVersion} snapshot ${manifest.pluginRevision}`)
} else {
  if (args.length !== 4 || args.some((arg, index) => index % 2 === 0 && !['--source', '--revision'].includes(arg))) throw new Error('Usage: node scripts/sync-dsivio-video-asr.mjs --source <plugin-directory> --revision <git-commit>; or --check')
  const source = path.resolve(argument('--source'))
  const revision = argument('--revision')
  if (!/^[a-f0-9]{40}$/.test(revision)) throw new Error('Revision must be a full lowercase Git commit hash')
  const actual = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim()
  if (actual !== revision) throw new Error(`Plugin HEAD does not match requested release revision`)
  const contents = Object.fromEntries(['server.py', 'prepare.py', 'requirements.txt'].map(name => [name, readFileSync(path.join(source, 'services/asr', name))]))
  const server = contents['server.py'].toString('utf8')
  if (!server.includes('SERVICE_VERSION = "0.2.0"') || !server.includes('PROTOCOL = "dsivio-video.asr/1"') || !contents['requirements.txt'].toString('utf8').includes('whisperx==3.8.6')) throw new Error('Unsupported ASR release versions')
  const files = Object.fromEntries(Object.entries(contents).map(([name, bytes]) => [name, createHash('sha256').update(bytes).digest('hex')]))
  // The source digest records the exact frozen working-tree snapshot too; a commit
  // alone cannot attest source edits prepared for a release without committing them.
  const sourceHash = createHash('sha256').update(JSON.stringify(files)).digest('hex')
  const manifest = { pluginRevision: `${revision}+asr-sha256:${sourceHash}`, protocol: 'dsivio-video.asr/1', serviceVersion: '0.2.0', whisperxVersion: '3.8.6', files }
  const destination = path.join(root, 'src-tauri/resources/dsivio-video-asr/0.2.0')
  mkdirSync(destination, { recursive: true })
  for (const [name, bytes] of Object.entries(contents)) writeFileSync(path.join(destination, name), bytes)
  const temporary = path.join(destination, 'manifest.json.part')
  writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`)
  renameSync(temporary, path.join(destination, 'manifest.json'))
  console.log(`Synced ASR ${manifest.serviceVersion}: ${manifest.pluginRevision}`)
}
