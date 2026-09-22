import type { WorkflowNode, WorkflowNodeKind } from './workflowModel'

import type { WorkflowConfig, WorkflowAsset } from '../../../generated/generationWorkflow'
export type { WorkflowConfig, WorkflowAsset, WorkflowModelChoice } from '../../../generated/generationWorkflow'

/** Defaults and JSON validation live here, including legacy drafts without config. */
export function defaultConfig(kind: WorkflowNodeKind): WorkflowConfig {
  if (['image.upload', 'image.uploadMany', 'video.upload'].includes(kind)) return { type: 'assets', assets: [] }
  if (kind === 'prompt.input') return { type: 'prompt', text: '' }
  if (['llm.text', 'prompt.optimize', 'text.join'].includes(kind)) return { type: 'text', instruction: kind === 'prompt.optimize' ? '优化以下提示词，使其清晰、具体、可执行。只输出优化后的提示词。' : '', text: '', model: null }
  if (kind === 'image.understand') return { type: 'understand', instruction: '分析参考图的构图、光线、背景、色彩和镜头角度，输出可用于商品图片复刻的详细提示词。保留新商品的外形、标识和材质，不复制参考商品本身。', assets: [], model: null }
  if (kind === 'image.generate' || kind === 'video.generate') return { type: 'generate', prompt: '', assets: [], model: null, options: {} }
  if (['text.preview', 'image.preview', 'image.download', 'image.previewMany', 'image.downloadZip', 'video.preview', 'video.download'].includes(kind)) return { type: 'output' }
  return { type: 'placeholder' }
}
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value)
export function parseConfig(kind: WorkflowNodeKind, value: unknown): WorkflowConfig | null {
  const config = defaultConfig(kind)
  if (value === undefined) return config
  if (!record(value)) return null
  if (value.type === 'placeholder' && config.type === 'text') return config
  if (value.type !== config.type) return null
  if ('assets' in config) {
    if (!Array.isArray(value.assets)) return null
    const assets: WorkflowAsset[] = []
    for (const item of value.assets) {
      if (!record(item) || typeof item.path !== 'string' || typeof item.name !== 'string' || typeof item.description !== 'string') return null
      // Object/data URLs are never durable references. Keep the description for reselection.
      assets.push({ path: /^(blob:|data:)/i.test(item.path) ? '' : item.path, name: item.name, description: item.description })
    }
    config.assets = assets
  }
  if ('model' in config) {
    if (value.model !== null && (!record(value.model) || typeof value.model.providerId !== 'string' || typeof value.model.model !== 'string')) return null
    config.model = value.model === null ? null : { providerId: (value.model as Record<string, string>).providerId, model: (value.model as Record<string, string>).model }
  }
  if (config.type === 'text') { if (typeof value.text !== 'string' || typeof value.instruction !== 'string') return null; config.text = value.text; config.instruction = value.instruction }
  if (config.type === 'prompt') { if (typeof value.text !== 'string') return null; config.text = value.text }
  if (config.type === 'understand') { if (typeof value.instruction !== 'string') return null; config.instruction = value.instruction }
  if (config.type === 'generate') {
    if (typeof value.prompt !== 'string' || !record(value.options) || Object.values(value.options).some(v => typeof v !== 'string')) return null
    config.prompt = value.prompt; config.options = { ...value.options } as Record<string, string>
  }
  return config
}
export const nodeConfig = (node: WorkflowNode): WorkflowConfig => node.config ?? defaultConfig(node.kind)
