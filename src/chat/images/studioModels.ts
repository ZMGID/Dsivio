import { normalizeProviderApiFormat, type ModelProvider } from '../../api/tauri'
import { resolveModelInfo } from '../../data/modelMatching'

export function isImageGenerationModel(provider: ModelProvider, model: string): boolean {
  return resolveModelInfo(model, provider.modelOverrides, provider).capabilities?.imageGeneration === true
}

export function isVisionModel(provider: ModelProvider, model: string): boolean {
  return resolveModelInfo(model, provider.modelOverrides, provider).capabilities?.vision === true
}

/** Studio engine still stores a protocol; adapters already know the route, so the UI does not ask. */
export function inferImageStudioProtocol(provider: ModelProvider | undefined, model: string): string {
  const name = model.toLowerCase()
  const format = normalizeProviderApiFormat(provider?.apiFormat)
  const base = (provider?.baseUrl || '').toLowerCase()
  if (format === 'gemini') return 'gemini'
  if (format === 'xai_responses' || name.includes('grok-imagine') || base.includes('api.x.ai')) {
    return 'grok'
  }
  if (
    (name.includes('gemini') && name.includes('image'))
    || name.includes('nano-banana')
    || name.startsWith('imagen')
  ) {
    return 'gemini-chat'
  }
  return 'openai'
}
