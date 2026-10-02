import type { DefaultModelSelection, ModelProvider, Settings, WorkbenchMediaConfig } from '../api/tauri'
import { resolveModelInfo } from './modelMatching'
import { isVideoGenerationModel } from './videoModels'
import { isSpeechModelConfigured, LOCAL_TRANSCRIBE_MODEL } from './speechModels'

export type MediaPoolKind = 'imageModels' | 'videoModels' | 'speechModels' | 'transcribeModels'
export const MEDIA_KINDS = ['imageModels', 'videoModels', 'speechModels', 'transcribeModels'] as const
export const MEDIA_KIND_LABEL: Record<MediaPoolKind, { zh: string; en: string }> = {
  imageModels: { zh: '图片', en: 'Image' },
  videoModels: { zh: '视频', en: 'Video' },
  speechModels: { zh: '语音', en: 'Speech' },
  transcribeModels: { zh: '转写', en: 'Transcription' },
}
export function mediaModelKey(selection: DefaultModelSelection): string {
  return JSON.stringify([selection.providerId, selection.model])
}
export function mediaModelName(provider: ModelProvider | undefined, model: string): string {
  return provider?.request.comfy?.workflows.find(w => w.id === model)?.name || model
}
export function isMediaPoolCandidate(provider: ModelProvider, model: string, kind: MediaPoolKind): boolean {
  if (kind === 'speechModels' || kind === 'transcribeModels') return provider.enabled !== false && provider.enabledModels.includes(model) && isSpeechModelConfigured(provider, model, kind)
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
      name: selection.providerId === 'local' ? 'WhisperX small' : mediaModelName(provider, selection.model),
      label: selection.providerId === 'local' ? 'Local / WhisperX small' : `${provider?.name || selection.providerId} / ${mediaModelName(provider, selection.model)}`,
      available: kind === 'transcribeModels' && selection.providerId === LOCAL_TRANSCRIBE_MODEL.providerId && selection.model === LOCAL_TRANSCRIBE_MODEL.model
        || Boolean(provider && isMediaPoolCandidate(provider, selection.model, kind)),
    }
  })
}
/** Draft cleanup for explicit deletion; backend sanitization is authoritative at persistence. */
export function removeMediaPoolEntries(config: WorkbenchMediaConfig, providerId: string, model?: string): WorkbenchMediaConfig {
  const keep = (entry: DefaultModelSelection) => entry.providerId !== providerId || (model !== undefined && entry.model !== model)
  return {
    ...config,
    imageModels: (config?.imageModels ?? []).filter(keep),
    videoModels: (config?.videoModels ?? []).filter(keep),
    speechModels: (config?.speechModels ?? []).filter(keep),
    transcribeModels: (config?.transcribeModels ?? []).filter(keep),
    localAsr: config?.localAsr,
  }
}
