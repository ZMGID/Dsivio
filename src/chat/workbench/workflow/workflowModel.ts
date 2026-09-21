export type WorkflowPortKind = 'image' | 'text' | 'video' | 'mesh'

export type WorkflowNodeKind =
  | 'image.upload'
  | 'image.uploadMany'
  | 'video.upload'
  | 'prompt.input'
  | 'text.if'
  | 'text.extract'
  | 'image.crop'
  | 'image.convert'
  | 'image.resize'
  | 'flow.loop'
  | 'llm.text'
  | 'text.join'
  | 'prompt.optimize'
  | 'image.understand'
  | 'image.generate'
  | 'video.generate'
  | 'mesh.generate'
  | 'text.preview'
  | 'image.preview'
  | 'image.download'
  | 'image.previewMany'
  | 'image.downloadZip'
  | 'video.preview'
  | 'video.download'
  | 'mesh.download'

export interface WorkflowPort {
  id: string
  kind: WorkflowPortKind
}

export interface WorkflowNode {
  id: string
  kind: WorkflowNodeKind
  title: string
  position: { x: number; y: number }
  note?: string
}

export interface WorkflowEdge {
  id: string
  source: string
  target: string
  sourceHandle?: string
  targetHandle?: string
}

export interface GenerationWorkflow {
  id: string
  name: string
  nodes: WorkflowNode[]
  edges: WorkflowEdge[]
  createdAt: string
  updatedAt: string
}

export function blankWorkflow(name: string): GenerationWorkflow {
  const now = new Date().toISOString()
  return {
    id: crypto.randomUUID(),
    name,
    nodes: [],
    edges: [],
    createdAt: now,
    updatedAt: now,
  }
}
