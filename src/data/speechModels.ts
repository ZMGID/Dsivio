import type { ModelInfo, ModelProvider } from '../api/tauri'

/** Product suggestions only. Choosing a provider or model never enables a speech route. */
export const SPEECH_MODELS = [
  { id: 'speech-2.8-hd', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-2.8-turbo', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-2.6-hd', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-2.6-turbo', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-02-hd', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-02-turbo', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-01-hd', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'speech-01-turbo', protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1' },
  { id: 'gpt-4o-mini-tts', protocol: 'openai_tts', baseUrl: 'https://api.openai.com/v1' },
  { id: 'tts-1', protocol: 'openai_tts', baseUrl: 'https://api.openai.com/v1' },
  { id: 'tts-1-hd', protocol: 'openai_tts', baseUrl: 'https://api.openai.com/v1' },
] as const
export const TRANSCRIBE_MODELS = [{ id: 'whisper-1', protocol: 'openai_transcribe', baseUrl: 'https://api.openai.com/v1' }] as const
export const LOCAL_TRANSCRIBE_MODEL = { providerId: 'local', model: 'whisperx-small' } as const

export function suggestedSpeechModels(baseUrl: string): string[] {
  let host: string
  try { host = new URL(baseUrl).hostname } catch { return [] }
  // Endpoint matching is only a picker hint, never credential or protocol inference.
  if (host === 'api.openai.com') return [...SPEECH_MODELS.filter(model => model.protocol === 'openai_tts').map(model => model.id), 'whisper-1']
  if (host === 'api.minimax.io' || host === 'api.minimaxi.com') return SPEECH_MODELS.filter(model => model.protocol === 'minimax_tts').map(model => model.id)
  return []
}

export function isSpeechModelConfigured(provider: ModelProvider, model: string, kind: 'speechModels' | 'transcribeModels'): boolean {
  const info: ModelInfo | undefined = provider.modelOverrides?.[model]
  const protocol = kind === 'speechModels' ? info?.speechProtocol : info?.transcribeProtocol
  const baseUrl = kind === 'speechModels' ? info?.speechBaseUrl : info?.transcribeBaseUrl
  const catalog = kind === 'speechModels' ? SPEECH_MODELS : TRANSCRIBE_MODELS
  return Boolean(baseUrl?.trim() && catalog.some(item => item.id === model && item.protocol === protocol))
}
