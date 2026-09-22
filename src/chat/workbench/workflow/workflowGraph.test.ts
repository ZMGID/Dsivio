import { describe, expect, it } from 'vitest'
import { i18n } from '../../../components/i18n'
import { imageReplicaTemplate } from './workflowCatalog'
import { defaultConfig, nodeConfig } from './workflowConfig'
import { arrangeNodes, connectionProblem, duplicateNodes, graphProblems, inputSource, nodeProblems, removeSelection } from './workflowGraph'
import { parseWorkflow } from './workflowStore'

describe('workflow editing invariants', () => {
  it('rejects whitespace-only local prompts and analysis instructions but accepts connected inputs', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    const understand = flow.nodes[1], generate = flow.nodes[3]
    understand.config = { type: 'understand', assets: [], instruction: ' \n\t ', model: { providerId: 'p', model: 'vision' } }
    generate.config = { type: 'generate', assets: [], prompt: ' \n ', model: { providerId: 'p', model: 'image' }, options: {} }
    expect(nodeProblems(flow, understand)).toContain('补充分析指令或连接上游')
    expect(nodeProblems({ ...flow, edges: [] }, generate)).toContain('补充生成提示词或连接上游')
    expect(nodeProblems(flow, generate)).not.toContain('补充生成提示词或连接上游')
    const source = { id: 'instruction', kind: 'prompt.input' as const, title: '指令', position: { x: 0, y: 0 }, config: { type: 'prompt' as const, text: '分析布局' } }
    const connected = { ...flow, nodes: [...flow.nodes, source], edges: [...flow.edges, { id: 'instruction-edge', source: source.id, sourceHandle: 'text', target: understand.id, targetHandle: 'prompt' }] }
    expect(nodeProblems(connected, understand)).not.toContain('补充分析指令或连接上游')
  })
  it('round-trips the configured replica without losing names, notes, assets, model options, positions or edges', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    flow.nodes[0] = { ...flow.nodes[0], title: '新参考图', note: '保留光线', config: { type: 'assets', assets: [{ path: '/tmp/ref.png', name: 'ref.png', description: '暖光参考' }] } }
    flow.nodes[1].config = { type: 'understand', assets: [], instruction: '分析背景和光线', model: { providerId: 'p', model: 'vision' } }
    flow.nodes[2].config = { type: 'assets', assets: [{ path: '/tmp/product.png', name: 'product.png', description: '红色背包' }] }
    flow.nodes[3].config = { type: 'generate', assets: [], prompt: '断线后的备用提示', model: { providerId: 'p', model: 'image' }, options: { output: '1:1:1k' } }
    flow.nodes[3].position = { x: 123, y: 456 }
    expect(parseWorkflow(JSON.parse(JSON.stringify(flow)))).toEqual(flow)
    expect(graphProblems(flow)).toEqual([])
    expect(inputSource(flow, flow.nodes[3].id, 'prompt')?.id).toBe(flow.nodes[1].id)
    const disconnected = { ...flow, edges: flow.edges.filter(e => e.targetHandle !== 'prompt') }
    expect(inputSource(disconnected, flow.nodes[3].id, 'prompt')).toBeUndefined()
    expect(nodeConfig(disconnected.nodes[3])).toMatchObject({ prompt: '断线后的备用提示' })
    expect(graphProblems(disconnected)).toEqual([])
  })
  it('copies configuration deeply, excludes edges outside the selection, and deletes incident edges', () => {
    const flow = imageReplicaTemplate(i18n.zh), copied = duplicateNodes(flow, [flow.nodes[0].id])
    expect(copied.nodes).toHaveLength(6); expect(copied.edges).toEqual(flow.edges)
    expect(copied.nodes[5].id).not.toBe(flow.nodes[0].id)
    copied.nodes[5].config = { type: 'assets', assets: [{ path: '/copy.png', name: 'copy', description: 'copy' }] }
    expect(nodeConfig(flow.nodes[0])).toEqual({ type: 'assets', assets: [] })
    const removed = removeSelection(flow, [flow.nodes[1].id], [])
    expect(removed.nodes).toHaveLength(4); expect(removed.edges).toHaveLength(2)
  })
  it('rejects invalid handles, self-links, duplicates, occupied inputs, mismatched collections and cycles', () => {
    const flow = imageReplicaTemplate(i18n.zh), edge = flow.edges[0]
    expect(connectionProblem(flow, edge)).toContain('已存在')
    expect(connectionProblem(flow, { ...edge, sourceHandle: 'wrong' })).toContain('端口不存在')
    expect(connectionProblem(flow, { ...edge, source: edge.target })).toContain('自身')
    expect(connectionProblem(flow, { ...edge, source: flow.nodes[2].id })).toContain('一条线')
    flow.nodes[0].kind = 'image.uploadMany'
    expect(connectionProblem({ ...flow, edges: [] }, edge)).toContain('单张与多张')
    const cycle = { ...flow.edges[0], source: flow.nodes[3].id, target: flow.nodes[1].id, sourceHandle: 'image', targetHandle: 'image' }
    expect(connectionProblem({ ...flow, edges: flow.edges.slice(1) }, cycle)).toContain('环')
  })
  it('keeps legacy placeholders, supplies defaults, and rejects malformed supported configs rather than silently dropping them', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    flow.nodes[0].kind = 'flow.loop'; delete flow.nodes[0].config
    expect(parseWorkflow(flow)?.nodes[0].config).toEqual(defaultConfig('flow.loop'))
    expect(parseWorkflow({ ...flow, nodes: [{ ...flow.nodes[1], config: { type: 'understand', instruction: 42 } }] })).toBeNull()
    expect(parseWorkflow({ ...flow, nodes: [flow.nodes[0], flow.nodes[0]] })).toBeNull()
    expect(parseWorkflow({ ...flow, nodes: [{ ...flow.nodes[0], position: { x: Infinity, y: 2 } }] })).toBeNull()
  })
  it('does not persist ephemeral URLs as usable assets', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    flow.nodes[0].config = { type: 'assets', assets: [{ path: 'blob:session-only', name: '原图', description: '需要重选' }] }
    expect(parseWorkflow(flow)?.nodes[0].config).toEqual({ type: 'assets', assets: [{ path: '', name: '原图', description: '需要重选' }] })
  })
})

it('copies a selected subgraph with its own connections and arranges dependencies left to right', () => {
  const flow = imageReplicaTemplate(i18n.zh)
  const copied = duplicateNodes(flow, [flow.nodes[0].id, flow.nodes[1].id])
  expect(copied.edges).toHaveLength(flow.edges.length + 1)
  expect(copied.edges.at(-1)).toMatchObject({ source: copied.nodes[5].id, target: copied.nodes[6].id })
  expect(copied.edges.at(-1)?.id).not.toBe(flow.edges[0].id)
  const arranged = arrangeNodes(flow)
  expect(arranged.edges).toEqual(flow.edges)
  for (const edge of arranged.edges) expect(arranged.nodes.find(n => n.id === edge.source)!.position.x).toBeLessThan(arranged.nodes.find(n => n.id === edge.target)!.position.x)
  expect(new Set(arranged.nodes.map(n => JSON.stringify(n.position))).size).toBe(arranged.nodes.length)
})
