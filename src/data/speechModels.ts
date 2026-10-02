import type { ModelInfo, ModelProvider } from '../api/tauri'
import { videoModel } from './videoModels'

/**
 * Catalog of speech and cloud-transcription models Dsivio can actually drive.
 * Every id here is implemented by the Rust adapters (`speech_providers.rs`,
 * `transcribe_providers.rs`); keep the two in step. Matching a catalog host only
 * pre-fills the connection fields. It never infers key permission: a provider key
 * does not prove the speech product is enabled for the account.
 */
export type SpeechProtocol = 'openai_tts' | 'minimax_tts'
export type TranscribeProtocol = 'openai_transcribe'

const MINIMAX_SPEECH_IDS = ['speech-2.8-hd', 'speech-2.8-turbo', 'speech-2.6-hd', 'speech-2.6-turbo', 'speech-02-hd', 'speech-02-turbo', 'speech-01-hd', 'speech-01-turbo']
const OPENAI_SPEECH_IDS = ['gpt-4o-mini-tts', 'gpt-4o-mini-tts-2025-12-15', 'tts-1', 'tts-1-hd']

/** Hosts whose official speech endpoint is known. `baseUrl` is the product URL to pre-fill. */
export const SPEECH_ENDPOINTS: Array<{ hosts: string[]; protocol: SpeechProtocol; baseUrl: string; models: string[] }> = [
  { hosts: ['api.minimax.io'], protocol: 'minimax_tts', baseUrl: 'https://api.minimax.io/v1', models: MINIMAX_SPEECH_IDS },
  { hosts: ['api.minimax.cn'], protocol: 'minimax_tts', baseUrl: 'https://api.minimax.cn/v1', models: MINIMAX_SPEECH_IDS },
  { hosts: ['api.openai.com'], protocol: 'openai_tts', baseUrl: 'https://api.openai.com/v1', models: OPENAI_SPEECH_IDS },
]
export const TRANSCRIBE_ENDPOINTS: Array<{ hosts: string[]; protocol: TranscribeProtocol; baseUrl: string; models: string[] }> = [
  { hosts: ['api.openai.com'], protocol: 'openai_transcribe', baseUrl: 'https://api.openai.com/v1', models: ['whisper-1'] },
]

/**
 * Speech-only products that do not serve a chat `/models` list. The picker shows these catalog ids
 * instead of fetching, exactly like the catalog-only video providers.
 */
export const SPEECH_PROVIDER_PRESETS = [
  { name: 'MiniMax 语音 · 国际站 (按量付费)', baseUrl: 'https://api.minimax.io/v1', apiKeyUrl: 'https://platform.minimax.io/user-center/basic-information/interface-key', catalogOnly: true },
  { name: 'MiniMax 语音 · 国内站 (按量付费)', baseUrl: 'https://api.minimax.cn/v1', apiKeyUrl: 'https://platform.minimaxi.com/user-center/basic-information/interface-key', catalogOnly: true },
] as const
export function isSpeechCatalogOnly(baseUrl: string): boolean {
  const base = baseUrl.trim().replace(/\/+$/, '')
  return SPEECH_PROVIDER_PRESETS.some(preset => preset.baseUrl === base)
}

/** Kept for tests and pickers that list every known id with its default endpoint. */
export const SPEECH_MODELS = SPEECH_ENDPOINTS.flatMap(endpoint => endpoint.models.map(id => ({ id, protocol: endpoint.protocol, baseUrl: endpoint.baseUrl })))
export const TRANSCRIBE_MODELS = TRANSCRIBE_ENDPOINTS.flatMap(endpoint => endpoint.models.map(id => ({ id, protocol: endpoint.protocol, baseUrl: endpoint.baseUrl })))
export const LOCAL_TRANSCRIBE_MODEL = { providerId: 'local', model: 'whisperx-small' } as const

function hostOf(baseUrl: string): string | null {
  try { return new URL(baseUrl).hostname.toLowerCase() } catch { return null }
}

function endpointFor<T extends { hosts: string[]; models: string[] }>(endpoints: T[], baseUrl: string, model: string): T | undefined {
  const host = hostOf(baseUrl)
  return host ? endpoints.find(endpoint => endpoint.hosts.includes(host) && endpoint.models.includes(model)) : undefined
}

/** Speech and transcription ids offered for an official host, for the model picker. */
export function suggestedSpeechModels(baseUrl: string): string[] {
  const host = hostOf(baseUrl)
  if (!host) return []
  return [...SPEECH_ENDPOINTS, ...TRANSCRIBE_ENDPOINTS]
    .filter(endpoint => endpoint.hosts.includes(host))
    .flatMap(endpoint => endpoint.models)
}

/**
 * The model info a known speech/transcription model starts with: its protocol, product URL and
 * display capabilities. Returns null for anything outside the catalog or on another host.
 */
export function knownAudioModelInfo(baseUrl: string, model: string): ModelInfo | null {
  const speech = endpointFor(SPEECH_ENDPOINTS, baseUrl, model)
  if (speech) return { speechProtocol: speech.protocol, speechBaseUrl: speech.baseUrl, capabilities: { speechGeneration: true } }
  const transcribe = endpointFor(TRANSCRIBE_ENDPOINTS, baseUrl, model)
  if (transcribe) return { transcribeProtocol: transcribe.protocol, transcribeBaseUrl: transcribe.baseUrl, capabilities: { speechTranscription: true } }
  return null
}

export function isSpeechModel(provider: Pick<ModelProvider, 'modelOverrides'> & { baseUrl?: string }, model: string): boolean {
  const info = provider.modelOverrides?.[model]
  return Boolean(info?.speechProtocol || info?.capabilities?.speechGeneration
    || (provider.baseUrl && endpointFor(SPEECH_ENDPOINTS, provider.baseUrl, model)))
}
export function isTranscribeModel(provider: Pick<ModelProvider, 'modelOverrides'> & { baseUrl?: string }, model: string): boolean {
  const info = provider.modelOverrides?.[model]
  return Boolean(info?.transcribeProtocol || info?.capabilities?.speechTranscription
    || (provider.baseUrl && endpointFor(TRANSCRIBE_ENDPOINTS, provider.baseUrl, model)))
}
/** A model that only makes or reads audio is not a chat model. */
export function isAudioOnlyModel(provider: Pick<ModelProvider, 'modelOverrides'> & { baseUrl: string }, model: string): boolean {
  return isSpeechModel(provider, model) || isTranscribeModel(provider, model)
}

/** Models that only produce or read media are never chat models. */
export function isNonChatModel(model: string, provider: Pick<ModelProvider, 'modelOverrides' | 'baseUrl'>): boolean {
  const info = provider.modelOverrides?.[model]
  return (info?.capabilities?.videoGeneration ?? Boolean(videoModel(model) || info?.videoProtocol)) || isAudioOnlyModel(provider, model)
}

/** Pool membership requires explicit configuration, never the chat key or the host alone. */
export function isSpeechModelConfigured(provider: ModelProvider, model: string, kind: 'speechModels' | 'transcribeModels'): boolean {
  const info: ModelInfo | undefined = provider.modelOverrides?.[model]
  const protocol = kind === 'speechModels' ? info?.speechProtocol : info?.transcribeProtocol
  const baseUrl = kind === 'speechModels' ? info?.speechBaseUrl : info?.transcribeBaseUrl
  const endpoints: Array<{ protocol: string; models: string[] }> = kind === 'speechModels' ? SPEECH_ENDPOINTS : TRANSCRIBE_ENDPOINTS
  return Boolean(baseUrl?.trim() && endpoints.some(endpoint => endpoint.protocol === protocol && endpoint.models.includes(model)))
}
