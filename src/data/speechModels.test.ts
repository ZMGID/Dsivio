import { describe, expect, it } from 'vitest'
import rust from '../../src-tauri/src/media_generation/speech_providers.rs?raw'
import { isAudioOnlyModel, isNonChatModel, isSpeechCatalogOnly, isSpeechModelConfigured, knownAudioModelInfo, suggestedSpeechModels, SPEECH_ENDPOINTS, TRANSCRIBE_ENDPOINTS } from './speechModels'
import { applyProviderDraftIntent } from '../settings/providerDraftIntents'
import { makeProvider, makeSettings } from '../settings/tabs/testFixtures'
import { buildModelPairOptions } from '../settings/utils'
import { PROVIDER_PRESETS } from '../settings/providerPresets'

describe('speech and transcription catalog', () => {
  it('lists exactly the models the Rust adapters implement', () => {
    const ids = (name: string) => [...rust.match(new RegExp(`const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\]`))![1].matchAll(/"([^"]+)"/g)].map(m => m[1]).sort()
    const catalog = (protocol: string) => [...new Set(SPEECH_ENDPOINTS.filter(e => e.protocol === protocol).flatMap(e => e.models))].sort()
    expect(catalog('minimax_tts')).toEqual(ids('MINIMAX_MODELS'))
    expect(catalog('openai_tts')).toEqual(ids('OPENAI_MODELS'))
    expect(TRANSCRIBE_ENDPOINTS.flatMap(e => e.models)).toEqual(['whisper-1'])
  })

  it('pre-fills the endpoint of the host the provider really uses, never another host', () => {
    expect(knownAudioModelInfo('https://api.minimax.cn/v1', 'speech-2.8-hd')).toMatchObject({ speechProtocol: 'minimax_tts', speechBaseUrl: 'https://api.minimax.cn/v1' })
    expect(knownAudioModelInfo('https://api.minimax.io/v1', 'speech-2.8-hd')).toMatchObject({ speechBaseUrl: 'https://api.minimax.io/v1' })
    expect(knownAudioModelInfo('https://api.openai.com/v1', 'whisper-1')).toMatchObject({ transcribeProtocol: 'openai_transcribe' })
    // A relay that serves the same model name is not the official product.
    expect(knownAudioModelInfo('https://relay.example/v1', 'tts-1')).toBeNull()
    expect(knownAudioModelInfo('https://api.openai.com/v1', 'gpt-4o')).toBeNull()
    expect(suggestedSpeechModels('https://relay.example/v1')).toEqual([])
    expect(suggestedSpeechModels('https://api.openai.com/v1')).toEqual(expect.arrayContaining(['tts-1', 'whisper-1']))
  })

  it('makes a known model selectable by adding it, and keeps what the user already set', () => {
    const provider = makeProvider({ baseUrl: 'https://api.openai.com/v1', enabledModels: [], modelOverrides: { 'tts-1': { speechBaseUrl: 'https://proxy.example/v1', speechProtocol: 'openai_tts' } } })
    const settings = makeSettings({ providers: [provider] })
    const added = applyProviderDraftIntent(settings, { type: 'add-models', id: 'p1', models: ['tts-1', 'whisper-1', 'gpt-4o'] }).providers[0]
    expect(isSpeechModelConfigured(added, 'whisper-1', 'transcribeModels')).toBe(true)
    expect(added.modelOverrides?.['whisper-1']?.capabilities?.speechTranscription).toBe(true)
    expect(added.modelOverrides?.['tts-1']?.speechBaseUrl).toBe('https://proxy.example/v1')
    expect(added.modelOverrides?.['tts-1']?.capabilities?.speechGeneration).toBe(true)
    expect(added.modelOverrides?.['gpt-4o']).toBeUndefined()
    const single = applyProviderDraftIntent(settings, { type: 'add-model', id: 'p1', model: 'gpt-4o-mini-tts' }).providers[0]
    expect(isSpeechModelConfigured(single, 'gpt-4o-mini-tts', 'speechModels')).toBe(true)
  })

  it('does not configure a model on an unknown host just because it was added', () => {
    const settings = makeSettings({ providers: [makeProvider({ baseUrl: 'https://relay.example/v1', enabledModels: [] })] })
    const added = applyProviderDraftIntent(settings, { type: 'add-model', id: 'p1', model: 'tts-1' }).providers[0]
    expect(isSpeechModelConfigured(added, 'tts-1', 'speechModels')).toBe(false)
    expect(added.modelOverrides?.['tts-1']).toBeUndefined()
  })

  it('keeps audio models out of chat model lists', () => {
    const provider = makeProvider({ baseUrl: 'https://api.openai.com/v1', enabledModels: ['gpt-4o', 'tts-1', 'whisper-1', 'custom-voice'], modelOverrides: { 'custom-voice': { speechProtocol: 'openai_tts' } } })
    expect(buildModelPairOptions([provider]).map(o => o.label)).toEqual(['OpenAI - gpt-4o'])
    expect(isNonChatModel('gpt-4o', provider)).toBe(false)
    expect(isAudioOnlyModel(provider, 'custom-voice')).toBe(true)
    // Same names on another host are ordinary chat ids unless the user configured them.
    expect(isNonChatModel('tts-1', makeProvider({ baseUrl: 'https://relay.example/v1' }))).toBe(false)
  })

  it('offers speech presets that skip the chat model fetch', () => {
    expect(isSpeechCatalogOnly('https://api.minimax.io/v1/')).toBe(true)
    expect(isSpeechCatalogOnly('https://api.openai.com/v1')).toBe(false)
    const names = PROVIDER_PRESETS.map(p => p.name)
    expect(names).toEqual(expect.arrayContaining(['MiniMax 语音 · 国际站 (按量付费)', 'MiniMax 语音 · 国内站 (按量付费)']))
  })
})
