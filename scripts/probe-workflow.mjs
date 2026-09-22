// Exercises the actual debug desktop command path without any network or model charges.
import fs from 'node:fs/promises'
import path from 'node:path'
import os from 'node:os'
import crypto from 'node:crypto'
import assert from 'node:assert/strict'
const data = process.platform === 'darwin' ? path.join(os.homedir(), 'Library/Application Support') : process.platform === 'win32' ? process.env.APPDATA : process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local/share')
const dir = path.join(data, 'com.zmair.kivio', 'chat_probe')
const id = crypto.randomUUID(), now = new Date().toISOString()
const workflow = { id, name: '工作流本机验收', createdAt: now, updatedAt: now, nodes: [
  { id: 'input', title: '商品内容', kind: 'prompt.input', position: { x: 0, y: 0 }, config: { type: 'prompt', text: '轻量骑行头盔' } },
  { id: 'join', title: '组合文案', kind: 'text.join', position: { x: 320, y: 0 }, config: { type: 'text', text: '备用值', instruction: '商品卖点', model: null } },
  { id: 'output', title: '输出', kind: 'text.preview', position: { x: 640, y: 0 }, config: { type: 'output' } },
], edges: [
  { id: 'a', source: 'input', target: 'join', sourceHandle: 'text', targetHandle: 'text' },
  { id: 'b', source: 'join', target: 'output', sourceHandle: 'text', targetHandle: 'text' },
] }
await fs.access(dir)
const requestPath = path.join(dir, 'request.json')
try { await fs.access(requestPath); throw new Error('Another desktop probe request is pending') } catch (error) { if (error.code !== 'ENOENT') throw error }
await fs.writeFile(path.join(dir, `workflow-${id}.tmp`), JSON.stringify({ id, prompt: '', workflowProbe: workflow }))
await fs.rename(path.join(dir, `workflow-${id}.tmp`), requestPath)
const deadline = Date.now() + 20000
while (Date.now() < deadline) {
  const response = await fs.readFile(path.join(dir, 'workflow-result.json'), 'utf8').then(JSON.parse).catch(() => null)
  if (response?.requestId === id && response.result?.Err) throw new Error(response.result.Err)
  if (response?.requestId === id && response.result?.Ok?.workflow?.id === id) {
    const run = response.result.Ok
    assert.equal(run.status, 'succeeded')
    assert.deepEqual(run.nodes.map(n => n.status), ['succeeded', 'succeeded', 'succeeded'])
    assert.equal(run.nodes[2].outputs.text.text, '商品卖点\n\n轻量骑行头盔')
    const saved = JSON.parse(await fs.readFile(path.join(data, 'com.zmair.kivio', 'workflow-runs', run.id, 'run.json'), 'utf8'))
    assert.equal(saved.status, 'succeeded')
    assert.equal(saved.nodes[2].outputs.text.text, run.nodes[2].outputs.text.text)
    console.log(JSON.stringify({ ok: true, runId: run.id, workflowId: id, output: run.nodes[2].outputs.text.text, persisted: true }))
    process.exit(0)
  }
  await new Promise(resolve => setTimeout(resolve, 150))
}
throw new Error('Workflow probe timed out. Start the updated debug desktop app first.')
