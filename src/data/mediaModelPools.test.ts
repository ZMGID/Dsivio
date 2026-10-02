import { describe, expect, it } from 'vitest'
import { isMediaPoolCandidate, mediaPoolEntries } from './mediaModelPools'
import { makeProvider, makeSettings } from '../settings/tabs/testFixtures'
import { applyProviderDraftIntent } from '../settings/providerDraftIntents'

describe('media pool boundary', () => {
  it('does not select every enabled model', () => {
    const settings=makeSettings({providers:[makeProvider({enabledModels:['MiniMax-H3']})]})
    expect(mediaPoolEntries(settings,'videoModels')).toEqual([])
  })
  it('preserves disabled membership, makes it unavailable, and removes deleted references', () => {
    const settings=makeSettings({providers:[makeProvider({enabled:false,enabledModels:['MiniMax-H3','grok-imagine-video']})],workbenchMedia:{imageModels:[],videoModels:[{providerId:'p1',model:'MiniMax-H3'},{providerId:'p1',model:'grok-imagine-video'}]}})
    expect(mediaPoolEntries(settings,'videoModels').every(m=>!m.available)).toBe(true)
    const removed=applyProviderDraftIntent(settings,{type:'remove-model',id:'p1',model:'MiniMax-H3'})
    expect(removed.workbenchMedia.videoModels).toEqual([{providerId:'p1',model:'grok-imagine-video'}])
    expect(applyProviderDraftIntent(settings,{type:'delete',id:'p1'}).workbenchMedia.videoModels).toEqual([])
  })
  it('requires explicit speech product configuration and never uses chat credentials or protocol as proof', () => {
    const provider = makeProvider({ enabledModels: ['gpt-4o-mini-tts', 'whisper-1'], baseUrl: 'https://api.openai.com/v1' })
    expect(isMediaPoolCandidate(provider, 'gpt-4o-mini-tts', 'speechModels')).toBe(false)
    expect(isMediaPoolCandidate(provider, 'whisper-1', 'transcribeModels')).toBe(false)
    provider.modelOverrides = {
      'gpt-4o-mini-tts': { speechProtocol: 'openai_tts', speechBaseUrl: 'https://api.openai.com/v1' },
      'whisper-1': { transcribeProtocol: 'openai_transcribe', transcribeBaseUrl: 'https://api.openai.com/v1' },
    }
    expect(isMediaPoolCandidate(provider, 'gpt-4o-mini-tts', 'speechModels')).toBe(true)
    expect(isMediaPoolCandidate(provider, 'whisper-1', 'speechModels')).toBe(false)
    expect(isMediaPoolCandidate(provider, 'whisper-1', 'transcribeModels')).toBe(true)
    provider.enabled = false
    expect(isMediaPoolCandidate(provider, 'whisper-1', 'transcribeModels')).toBe(false)
  })
  it('keeps built-in local transcription available without a provider or a fake key after deleting cloud models', () => {
    const settings = makeSettings({ providers: [makeProvider({ enabledModels: ['whisper-1'] })], workbenchMedia: {
      speechModels: [{ providerId: 'p1', model: 'gpt-4o-mini-tts' }],
      transcribeModels: [{ providerId: 'local', model: 'whisperx-small' }, { providerId: 'p1', model: 'whisper-1' }],
    } })
    const cleaned = applyProviderDraftIntent(settings, { type: 'delete', id: 'p1' })
    expect(mediaPoolEntries(cleaned, 'transcribeModels')).toMatchObject([{ providerId: 'local', model: 'whisperx-small', available: true }])
    expect(cleaned.workbenchMedia.speechModels).toEqual([])
    expect(cleaned.workbenchMedia.localAsr).toEqual(settings.workbenchMedia.localAsr)
  })
})
