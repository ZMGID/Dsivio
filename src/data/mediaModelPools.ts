import type { DefaultModelSelection, ModelProvider, Settings, WorkbenchMediaConfig } from '../api/tauri'
import { resolveModelInfo } from './modelMatching'
import { isVideoGenerationModel } from './videoModels'

export type MediaPoolKind = keyof WorkbenchMediaConfig
export function mediaModelKey(selection: DefaultModelSelection): string {
  return JSON.stringify([selection.providerId, selection.model])
}
export function mediaModelName(provider: ModelProvider | undefined, model: string): string {
  return provider?.request.comfy?.workflows.find(w => w.id === model)?.name || model
}
export function isMediaPoolCandidate(provider: ModelProvider, model: string, kind: MediaPoolKind): boolean {
  if (provider.request?.comfy) return provider.enabled !== false && provider.request?.comfy.workflows.some(w => w.id === model && w.kind === (kind === 'imageModels' ? 'image' : 'video'))
  return provider.enabled !== false && provider.enabledModels.includes(model) && (kind === 'videoModels'
    ? isVideoGenerationModel(model, provider)
    : resolveModelInfo(model, provider.modelOverrides, provider).capabilities?.imageGeneration === true)
}
/** Resolve explicit pool membership only; conversation defaults never participate. */
export function mediaPoolEntries(settings: Pick<Settings, 'providers' | 'workbenchMedia'>, kind: MediaPoolKind) {
  return (settings.workbenchMedia?.[kind] ?? []).map(selection => {
    const provider = settings.providers.find(item => item.id === selection.providerId)
    return {
      ...selection,
      key: mediaModelKey(selection),
      name: mediaModelName(provider, selection.model),
      label: `${provider?.name || selection.providerId} / ${mediaModelName(provider, selection.model)}`,
      available: Boolean(provider && isMediaPoolCandidate(provider, selection.model, kind)),
    }
  })
}
/** Draft cleanup for explicit deletion; backend sanitization is authoritative at persistence. */
export function removeMediaPoolEntries(config: WorkbenchMediaConfig | undefined, providerId: string, model?: string): WorkbenchMediaConfig {
  const keep = (entry: DefaultModelSelection) => entry.providerId !== providerId || (model !== undefined && entry.model !== model)
  return { imageModels: (config?.imageModels ?? []).filter(keep), videoModels: (config?.videoModels ?? []).filter(keep) }
}
