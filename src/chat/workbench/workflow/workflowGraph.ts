import { nodeConfig } from './workflowConfig'
import { paletteEntry } from './workflowCatalog'
import type { GenerationWorkflow, WorkflowEdge, WorkflowNode } from './workflowModel'

/** Every input has one incoming edge; collection ports carry an array in that edge. */
export function connectionProblem(flow: GenerationWorkflow, edge: Omit<WorkflowEdge, 'id'>): string | null {
  const source = flow.nodes.find(n => n.id === edge.source), target = flow.nodes.find(n => n.id === edge.target)
  if (!source || !target) return '连接节点不存在'
  if (source.id === target.id) return '不能连接节点自身'
  const from = paletteEntry(source.kind)?.outputs.find(p => p.id === edge.sourceHandle)
  const to = paletteEntry(target.kind)?.inputs.find(p => p.id === edge.targetHandle)
  if (!from || !to) return '端口不存在，请重新连线'
  if (from.kind !== to.kind || !!from.many !== !!to.many) return '端口类型不匹配：单张与多张图片不能直接相连'
  if (flow.edges.some(e => e.source === edge.source && e.target === edge.target && e.sourceHandle === edge.sourceHandle && e.targetHandle === edge.targetHandle)) return '这条连接已存在'
  if (flow.edges.some(e => e.target === edge.target && e.targetHandle === edge.targetHandle)) return '每个输入只能接一条线，请先断开已有连接'
  const visited = new Set<string>(), queue = [edge.target]
  while (queue.length) {
    const id = queue.pop()!
    if (id === edge.source) return '普通数据流不允许形成环'
    if (visited.has(id)) continue
    visited.add(id)
    queue.push(...flow.edges.filter(e => e.source === id).map(e => e.target))
  }
  return null
}
export function inputSource(flow: GenerationWorkflow, nodeId: string, port: string): WorkflowNode | undefined {
  const edge = flow.edges.find(e => e.target === nodeId && e.targetHandle === port && !connectionProblem({ ...flow, edges: flow.edges.filter(other => other.id !== e.id) }, e))
  return flow.nodes.find(n => n.id === edge?.source)
}
export function nodeProblems(flow: GenerationWorkflow, node: WorkflowNode): string[] {
  const config = nodeConfig(node), problems: string[] = []
  if (config.type === 'placeholder') return ['此节点尚未完善，仅保留原草稿']
  if (!node.title.trim()) problems.push('填写节点名称')
  if (config.type === 'assets' && !config.assets.length) problems.push('选择素材')
  const localAssetsActive = config.type === 'assets' || !inputSource(flow, node.id, 'image')
  if (localAssetsActive && 'assets' in config && config.assets.some(a => !a.path)) problems.push('素材引用缺失，请重新选择')
  if (localAssetsActive && 'assets' in config && node.kind !== 'image.uploadMany' && config.assets.length > 1) problems.push('此输入只能选择一个素材')
  if (config.type === 'prompt' && !config.text.trim()) problems.push('填写提示词')
  if (node.kind !== 'text.join' && 'model' in config && (!config.model?.providerId || !config.model.model)) problems.push('选择模型')
  for (const port of paletteEntry(node.kind)?.inputs ?? []) {
    if (!port.required || inputSource(flow, node.id, port.id)) continue
    const local = port.id === 'text' && config.type === 'text' ? config.text.trim() : port.id === 'prompt' ? (config.type === 'understand' ? config.instruction : config.type === 'generate' ? config.prompt : '').trim() : port.id === 'image' && 'assets' in config ? config.assets.some(a => a.path) : false
    if (!local) problems.push(`补充${port.label}或连接上游`)
  }
  return problems
}
export function graphProblems(flow: GenerationWorkflow): string[] {
  const problems = flow.nodes.flatMap(node => nodeProblems(flow, node).map(message => `${node.title}：${message}`))
  const accepted: WorkflowEdge[] = []
  for (const edge of flow.edges) {
    const issue = connectionProblem({ ...flow, edges: accepted }, edge)
    if (issue) problems.push(issue)
    else accepted.push(edge)
  }
  if (!flow.name.trim()) problems.push('请填写工作流名称')
  if (!flow.nodes.length) problems.push('请添加节点')
  return problems
}
export function removeSelection(flow: GenerationWorkflow, nodes: string[], edges: string[]): GenerationWorkflow {
  return { ...flow, nodes: flow.nodes.filter(n => !nodes.includes(n.id)), edges: flow.edges.filter(e => !edges.includes(e.id) && !nodes.includes(e.source) && !nodes.includes(e.target)) }
}
export function duplicateNodes(flow: GenerationWorkflow, ids: string[]): GenerationWorkflow {
  const mapping = new Map(flow.nodes.filter(n => ids.includes(n.id)).map(n => [n.id, crypto.randomUUID()]))
  return { ...flow,
    nodes: [...flow.nodes, ...flow.nodes.filter(n => mapping.has(n.id)).map(n => ({ ...structuredClone(n), id: mapping.get(n.id)!, title: `${n.title} 副本`, position: { x: n.position.x + 48, y: n.position.y + 48 } }))],
    edges: [...flow.edges, ...flow.edges.filter(e => mapping.has(e.source) && mapping.has(e.target)).map(e => ({ ...e, id: crypto.randomUUID(), source: mapping.get(e.source)!, target: mapping.get(e.target)! }))],
  }
}

/** Stable DAG columns; legacy cyclic drafts remain visible in the final column. */
export function arrangeNodes(flow: GenerationWorkflow): GenerationWorkflow {
  const levels = new Map<string, number>()
  for (let pass = 0; pass < flow.nodes.length; pass++) {
    for (const node of flow.nodes) {
      if (levels.has(node.id)) continue
      const parents = flow.edges.filter(e => e.target === node.id).map(e => e.source)
      if (parents.every(id => levels.has(id))) levels.set(node.id, parents.length ? 1 + Math.max(...parents.map(id => levels.get(id)!)) : 0)
    }
  }
  const rows = new Map<number, number>()
  return { ...flow, nodes: flow.nodes.map(node => {
    const column = levels.get(node.id) ?? 0, row = rows.get(column) ?? 0
    rows.set(column, row + 1)
    return { ...node, position: { x: 48 + column * 320, y: 48 + row * 300 } }
  }) }
}
