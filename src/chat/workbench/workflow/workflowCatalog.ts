import { defaultConfig } from './workflowConfig'
import type { I18n } from '../../../components/i18n'
import { blankWorkflow, type GenerationWorkflow, type WorkflowNodeKind, type WorkflowPort } from './workflowModel'

export type PaletteGroupId = 'input' | 'tool' | 'text' | 'image' | 'video' | 'mesh' | 'output'

export interface PaletteNode {
  kind: WorkflowNodeKind
  group: PaletteGroupId
  label: (t: I18n) => string
  hint: (t: I18n) => string
  inputs: WorkflowPort[]
  outputs: WorkflowPort[]
}

/** 节点库按参考站画布左侧分组抄：输入 / 工具 / 文本 / 图片 / 视频 / 3D / 输出。 */
export const WORKFLOW_PALETTE: readonly PaletteNode[] = [
  { kind: 'image.uploadMany', group: 'input', label: (t) => t.wfNodeImageUploadMany, hint: (t) => t.wfNodeImageUploadManyHint, inputs: [], outputs: [{ id: 'image', kind: 'image', label: '多张图片', many: true }] },
  { kind: 'image.upload', group: 'input', label: (t) => t.wfNodeImageUpload, hint: (t) => t.wfNodeImageUploadHint, inputs: [], outputs: [{ id: 'image', kind: 'image', label: '图片素材' }] },
  { kind: 'video.upload', group: 'input', label: (t) => t.wfNodeVideoUpload, hint: (t) => t.wfNodeVideoUploadHint, inputs: [], outputs: [{ id: 'video', kind: 'video', label: '视频素材' }] },
  { kind: 'prompt.input', group: 'input', label: (t) => t.wfNodePrompt, hint: (t) => t.wfNodePromptHint, inputs: [], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'text.if', group: 'tool', label: (t) => t.wfNodeIf, hint: (t) => t.wfNodeIfHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'text.extract', group: 'tool', label: (t) => t.wfNodeExtract, hint: (t) => t.wfNodeExtractHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'image.crop', group: 'tool', label: (t) => t.wfNodeCrop, hint: (t) => t.wfNodeCropHint, inputs: [{ id: 'image', kind: 'image', label: '图片素材', required: true }], outputs: [{ id: 'image', kind: 'image', label: '图片素材' }] },
  { kind: 'image.convert', group: 'tool', label: (t) => t.wfNodeConvert, hint: (t) => t.wfNodeConvertHint, inputs: [{ id: 'image', kind: 'image', label: '图片素材', required: true }], outputs: [{ id: 'image', kind: 'image', label: '图片素材' }] },
  { kind: 'image.resize', group: 'tool', label: (t) => t.wfNodeResize, hint: (t) => t.wfNodeResizeHint, inputs: [{ id: 'image', kind: 'image', label: '图片素材', required: true }], outputs: [{ id: 'image', kind: 'image', label: '图片素材' }] },
  { kind: 'flow.loop', group: 'tool', label: (t) => t.wfNodeLoop, hint: (t) => t.wfNodeLoopHint, inputs: [], outputs: [] },
  { kind: 'llm.text', group: 'text', label: (t) => t.wfNodeLlm, hint: (t) => t.wfNodeLlmHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'text.join', group: 'text', label: (t) => t.wfNodeJoin, hint: (t) => t.wfNodeJoinHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'prompt.optimize', group: 'text', label: (t) => t.wfNodePromptOpt, hint: (t) => t.wfNodePromptOptHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [{ id: 'text', kind: 'text', label: '文本内容' }] },
  { kind: 'image.understand', group: 'image', label: (t) => t.wfNodeUnderstand, hint: (t) => t.wfNodeUnderstandHint, inputs: [{ id: 'image', kind: 'image', label: '待分析参考图', required: true }, { id: 'prompt', kind: 'text', label: '分析指令', required: true }], outputs: [{ id: 'text', kind: 'text', label: '图片分析提示词' }] },
  { kind: 'image.generate', group: 'image', label: (t) => t.wfNodeGenerate, hint: (t) => t.wfNodeGenerateHint, inputs: [{ id: 'prompt', kind: 'text', label: '生成提示词', required: true }, { id: 'image', kind: 'image', label: '商品参考图' }], outputs: [{ id: 'image', kind: 'image', label: '生成图片' }] },
  { kind: 'video.generate', group: 'video', label: (t) => t.wfNodeVideoGen, hint: (t) => t.wfNodeVideoGenHint, inputs: [{ id: 'image', kind: 'image', label: '商品参考图' }, { id: 'prompt', kind: 'text', label: '生成提示词', required: true }], outputs: [{ id: 'video', kind: 'video', label: '生成视频' }] },
  { kind: 'mesh.generate', group: 'mesh', label: (t) => t.wfNodeMesh, hint: (t) => t.wfNodeMeshHint, inputs: [{ id: 'image', kind: 'image', label: '图片素材', required: true }], outputs: [{ id: 'mesh', kind: 'mesh', label: '3D 模型' }] },
  { kind: 'text.preview', group: 'output', label: (t) => t.wfNodeTextPreview, hint: (t) => t.wfNodeTextPreviewHint, inputs: [{ id: 'text', kind: 'text', label: '文本内容', required: true }], outputs: [] },
  { kind: 'image.preview', group: 'output', label: (t) => t.wfNodeImagePreview, hint: (t) => t.wfNodeImagePreviewHint, inputs: [{ id: 'image', kind: 'image', label: '待输出图片', required: true }], outputs: [] },
  { kind: 'image.download', group: 'output', label: (t) => t.wfNodeImageDownload, hint: (t) => t.wfNodeImageDownloadHint, inputs: [{ id: 'image', kind: 'image', label: '待输出图片', required: true }], outputs: [] },
  { kind: 'image.previewMany', group: 'output', label: (t) => t.wfNodeImagePreviewMany, hint: (t) => t.wfNodeImagePreviewManyHint, inputs: [{ id: 'image', kind: 'image', label: '多张图片', many: true, required: true }], outputs: [] },
  { kind: 'image.downloadZip', group: 'output', label: (t) => t.wfNodeImageZip, hint: (t) => t.wfNodeImageZipHint, inputs: [{ id: 'image', kind: 'image', label: '多张图片', many: true, required: true }], outputs: [] },
  { kind: 'video.preview', group: 'output', label: (t) => t.wfNodeVideoPreview, hint: (t) => t.wfNodeVideoPreviewHint, inputs: [{ id: 'video', kind: 'video', label: '待输出视频', required: true }], outputs: [] },
  { kind: 'video.download', group: 'output', label: (t) => t.wfNodeVideoDownload, hint: (t) => t.wfNodeVideoDownloadHint, inputs: [{ id: 'video', kind: 'video', label: '待输出视频', required: true }], outputs: [] },
  { kind: 'mesh.download', group: 'output', label: (t) => t.wfNodeMeshDownload, hint: (t) => t.wfNodeMeshDownloadHint, inputs: [{ id: 'mesh', kind: 'mesh', label: '3D 模型' }], outputs: [] },
]

export const PALETTE_GROUPS: { id: PaletteGroupId; label: (t: I18n) => string }[] = [
  { id: 'input', label: (t) => t.wfGroupInput },
  { id: 'tool', label: (t) => t.wfGroupTool },
  { id: 'text', label: (t) => t.wfGroupText },
  { id: 'image', label: (t) => t.wfGroupImage },
  { id: 'video', label: (t) => t.wfGroupVideo },
  { id: 'mesh', label: (t) => t.wfGroupMesh },
  { id: 'output', label: (t) => t.wfGroupOutput },
]

export function paletteEntry(kind: WorkflowNodeKind): PaletteNode | undefined {
  return WORKFLOW_PALETTE.find((item) => item.kind === kind)
}

export function canConnectPorts(from: WorkflowPort['kind'], to: WorkflowPort['kind']): boolean {
  return from === to
}

/**
 * 图片复刻：参考站模板实测 5 节点 / 4 边。
 * 参考图 → 图片理解 → 图片生成（提示词），产品图 → 图片生成，生成结果 → 单图下载。
 */
export function imageReplicaTemplate(t: I18n): GenerationWorkflow {
  const flow = blankWorkflow(t.workbenchTplReplicate)
  const ref = { id: crypto.randomUUID(), kind: 'image.upload' as const, title: t.wfTitleRefImage, position: { x: 40, y: 80 } }
  const understand = { id: crypto.randomUUID(), kind: 'image.understand' as const, title: t.wfNodeUnderstand, position: { x: 340, y: 80 } }
  const product = { id: crypto.randomUUID(), kind: 'image.upload' as const, title: t.wfTitleProductImage, position: { x: 40, y: 340 } }
  const generate = { id: crypto.randomUUID(), kind: 'image.generate' as const, title: t.wfNodeGenerate, position: { x: 340, y: 340 } }
  const download = { id: crypto.randomUUID(), kind: 'image.download' as const, title: t.wfNodeImageDownload, position: { x: 660, y: 340 } }
  flow.nodes = [ref, understand, product, generate, download].map(node => ({ ...node, config: defaultConfig(node.kind) }))
  flow.edges = [
    { id: crypto.randomUUID(), source: ref.id, target: understand.id, sourceHandle: 'image', targetHandle: 'image' },
    { id: crypto.randomUUID(), source: understand.id, target: generate.id, sourceHandle: 'text', targetHandle: 'prompt' },
    { id: crypto.randomUUID(), source: product.id, target: generate.id, sourceHandle: 'image', targetHandle: 'image' },
    { id: crypto.randomUUID(), source: generate.id, target: download.id, sourceHandle: 'image', targetHandle: 'image' },
  ]
  return flow
}

export function officialTemplates(t: I18n): { id: string; name: string; hint: string; nodes: number; edges: number; build: () => GenerationWorkflow }[] {
  return [
    {
      id: 'text-demo', name: '文本组合示例', hint: '无需配置模型，体验输入、连线和运行结果', nodes: 3, edges: 2,
      build: () => {
        const flow = blankWorkflow('文本组合示例')
        const input = crypto.randomUUID(), join = crypto.randomUUID(), preview = crypto.randomUUID()
        flow.nodes = [
          { id: input, kind: 'prompt.input', title: '商品内容', position: { x: 48, y: 48 }, config: { type: 'prompt', text: '轻量骑行头盔，透气舒适。' } },
          { id: join, kind: 'text.join', title: '组合文案', position: { x: 368, y: 48 }, config: { type: 'text', instruction: '商品卖点', text: '', model: null } },
          { id: preview, kind: 'text.preview', title: '文案结果', position: { x: 688, y: 48 }, config: { type: 'output' } },
        ]
        flow.edges = [
          { id: crypto.randomUUID(), source: input, target: join, sourceHandle: 'text', targetHandle: 'text' },
          { id: crypto.randomUUID(), source: join, target: preview, sourceHandle: 'text', targetHandle: 'text' },
        ]
        return flow
      },
    },
    {
      id: 'replicate',
      name: t.workbenchTplReplicate,
      hint: t.workbenchTplReplicateDesc,
      nodes: 5,
      edges: 4,
      build: () => imageReplicaTemplate(t),
    },
  ]
}
