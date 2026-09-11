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
  if (format === 'xai_responses' || name.startsWith('grok') || name.includes('grok-imagine') || hostMatches(base, 'api.x.ai')) {
    return 'grok'
  }
  if (
    (name.includes('gemini') && name.includes('image'))
    || name.includes('nano-banana')
    || name.startsWith('imagen')
  ) {
    return hostMatches(base, 'ybw-ai.com') ? 'gemini-chat' : 'gemini'
  }
  if (usesAsyncImageGateway(base, name)) return 'async'
  return 'openai'
}

function hostMatches(base: string, domain: string): boolean {
  try {
    const host = new URL(base.includes('://') ? base : `https://${base}`).hostname.toLowerCase()
    return host === domain || host.endsWith(`.${domain}`)
  } catch {
    return base.toLowerCase().includes(domain)
  }
}

/** Official OpenAI can hold a sync images call. Compatible gateways time out; they need submit + poll. */
function usesAsyncImageGateway(base: string, model: string): boolean {
  if (base.includes('apimart')) return true
  const officialOpenAI = base.includes('api.openai.com')
  const imagesApiModel = model.includes('gpt-image') || model.startsWith('dall-e')
  return imagesApiModel && !officialOpenAI
}

/** Stale `openai` configs for relay image models must flip to async; do not clobber other saved protocols. */
export function reconcileImageStudioProtocol<T extends { protocol: string; model: string }>(
  config: T,
  provider?: ModelProvider,
): T {
  const inferred = inferImageStudioProtocol(provider, config.model)
  if (inferred === 'grok' || inferred === 'gemini' || inferred === 'gemini-chat') {
    return config.protocol === inferred ? config : { ...config, protocol: inferred }
  }
  if (config.protocol === 'openai' && inferred === 'async') {
    return { ...config, protocol: 'async' }
  }
  return config
}
