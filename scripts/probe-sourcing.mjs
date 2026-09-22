// Local-only desktop smoke test. Does not submit images or consume 1688 quota.
import fs from 'node:fs/promises'
import path from 'node:path'
import os from 'node:os'
import crypto from 'node:crypto'
import assert from 'node:assert/strict'
const data = process.platform === 'darwin' ? path.join(os.homedir(), 'Library/Application Support') : process.platform === 'win32' ? process.env.APPDATA : process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local/share')
const dir = path.join(data, 'com.zmair.kivio', 'chat_probe')
const id = crypto.randomUUID()
const request = path.join(dir, 'request.json')
await fs.access(dir)
try { await fs.access(request); throw new Error('Another desktop probe is pending') } catch (e) { if (e.code !== 'ENOENT') throw e }
const temp = path.join(dir, `sourcing-${id}.tmp`)
await fs.writeFile(temp, JSON.stringify({ id, prompt: '', sourcingProbe: true }))
await fs.rename(temp, request)
const deadline = Date.now() + 20000
while (Date.now() < deadline) {
  const response = await fs.readFile(path.join(dir, 'sourcing-result.json'), 'utf8').then(JSON.parse).catch(() => null)
  if (response?.requestId === id) {
    if (response.result?.Err) throw new Error(response.result.Err)
    const result = response.result.Ok
    assert.equal(result.persisted, true)
    assert.equal(result.deduplicated, true)
    assert.equal(result.updated, true)
    assert.equal(result.staleRejected, true)
    assert.equal(result.filtered, true)
    if (!result.akConfigured) assert.match(result.missingAkError, /AK/)
    console.log(JSON.stringify({ ok: true, ...result }))
    process.exit(0)
  }
  await new Promise(resolve => setTimeout(resolve, 150))
}
throw new Error('Sourcing probe timed out. Start the updated debug desktop app first.')
