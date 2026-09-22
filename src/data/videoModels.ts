import catalog from './videoModelCatalog.json'
import type { ModelInfo, ModelProvider } from '../api/tauri'

/** Single catalog shared with the Rust video adapter via include_str!. */
import type { VideoProtocol } from '../generated/videoGeneration'
export type { VideoProtocol } from '../generated/videoGeneration'
export const VIDEO_PROTOCOLS = catalog.protocols
export const VIDEO_CATALOG_VERIFIED_AT = catalog.verifiedAt
export const VIDEO_PROVIDER_PRESETS = catalog.providers
export const VIDEO_MODELS = catalog.models

export function videoModel(model: string) {
  const id = model.trim().toLowerCase()
  return VIDEO_MODELS.find(item => item.id.toLowerCase() === id)
}

export function videoModelInfo(model: string): ModelInfo | null {
  const entry = videoModel(model)
  return entry ? {
    displayName: entry.label,
    videoProtocol: entry.protocol as VideoProtocol,
    capabilities: { videoGeneration: true, videoInput: false, vision: false, streaming: false, functionCalling: false },
  } : null
}

export function videoProvider(baseUrl: string) {
  const base = baseUrl.trim().replace(/\/+$/, '')
  return VIDEO_PROVIDER_PRESETS.find(item => item.baseUrl === base)
}

/** Official catalog suggestions are not an account-access check and never auto-enable models. */
export function suggestedVideoModels(baseUrl: string): string[] {
  const preset = videoProvider(baseUrl)
  return preset ? VIDEO_MODELS.filter(model => preset.protocols.includes(model.protocol)).map(model => model.id) : []
}

export function isVideoGenerationModel(model: string, provider?: Pick<ModelProvider, 'modelOverrides'>): boolean {
  return provider?.modelOverrides?.[model]?.capabilities?.videoGeneration
    ?? Boolean(videoModel(model) || provider?.modelOverrides?.[model]?.videoProtocol)
}
