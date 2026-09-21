import { blankWorkflow, type GenerationWorkflow, type WorkflowEdge, type WorkflowNode, type WorkflowNodeKind } from './workflowModel'

const KEY = 'kivio.workbench.workflows'

const KINDS = new Set<WorkflowNodeKind>([
  'image.upload', 'image.uploadMany', 'video.upload', 'prompt.input',
  'text.if', 'text.extract', 'image.crop', 'image.convert', 'image.resize', 'flow.loop',
  'llm.text', 'text.join', 'prompt.optimize',
  'image.understand', 'image.generate', 'video.generate', 'mesh.generate',
  'text.preview', 'image.preview', 'image.download', 'image.previewMany', 'image.downloadZip',
  'video.preview', 'video.download', 'mesh.download',
])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function parseNode(value: unknown): WorkflowNode | null {
  if (!isRecord(value)) return null
  if (typeof value.id !== 'string' || !value.id) return null
  if (typeof value.kind !== 'string' || !KINDS.has(value.kind as WorkflowNodeKind)) return null
  if (typeof value.title !== 'string') return null
  if (!isRecord(value.position) || typeof value.position.x !== 'number' || typeof value.position.y !== 'number') return null
  const node: WorkflowNode = {
    id: value.id,
    kind: value.kind as WorkflowNodeKind,
    title: value.title,
    position: { x: value.position.x, y: value.position.y },
  }
  if (typeof value.note === 'string') node.note = value.note
  return node
}

function parseEdge(value: unknown): WorkflowEdge | null {
  if (!isRecord(value)) return null
  if (typeof value.id !== 'string' || !value.id) return null
  if (typeof value.source !== 'string' || typeof value.target !== 'string') return null
  const edge: WorkflowEdge = { id: value.id, source: value.source, target: value.target }
  if (typeof value.sourceHandle === 'string') edge.sourceHandle = value.sourceHandle
  if (typeof value.targetHandle === 'string') edge.targetHandle = value.targetHandle
  return edge
}

export function parseWorkflow(value: unknown): GenerationWorkflow | null {
  if (!isRecord(value)) return null
  if (typeof value.id !== 'string' || !value.id) return null
  if (typeof value.name !== 'string') return null
  if (!Array.isArray(value.nodes) || !Array.isArray(value.edges)) return null
  const nodes = value.nodes.map(parseNode).filter((item): item is WorkflowNode => item !== null)
  const edges = value.edges.map(parseEdge).filter((item): item is WorkflowEdge => item !== null)
  const createdAt = typeof value.createdAt === 'string' ? value.createdAt : new Date().toISOString()
  const updatedAt = typeof value.updatedAt === 'string' ? value.updatedAt : createdAt
  return { id: value.id, name: value.name, nodes, edges, createdAt, updatedAt }
}

function storage(): Storage | null {
  try {
    return globalThis.localStorage
  } catch {
    return null
  }
}

function readAll(): GenerationWorkflow[] {
  try {
    const raw = storage()?.getItem(KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed.map(parseWorkflow).filter((item): item is GenerationWorkflow => item !== null)
  } catch {
    return []
  }
}

function writeAll(items: GenerationWorkflow[]): void {
  storage()?.setItem(KEY, JSON.stringify(items))
}

export const workflowStore = {
  list(): GenerationWorkflow[] {
    return readAll().sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
  },
  get(id: string): GenerationWorkflow | null {
    return readAll().find((item) => item.id === id) ?? null
  },
  save(flow: GenerationWorkflow): GenerationWorkflow {
    const next = { ...flow, updatedAt: new Date().toISOString() }
    const items = readAll().filter((item) => item.id !== flow.id)
    items.push(next)
    writeAll(items)
    return next
  },
  remove(id: string): void {
    writeAll(readAll().filter((item) => item.id !== id))
  },
  create(name: string): GenerationWorkflow {
    return workflowStore.save(blankWorkflow(name))
  },
}
