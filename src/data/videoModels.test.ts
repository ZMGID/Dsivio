import { describe, expect, it } from 'vitest'
import { VIDEO_MODELS, VIDEO_PROTOCOLS, suggestedVideoModels, isVideoGenerationModel } from './videoModels'
import { resolveModelInfo } from './modelMatching'
import { PROVIDER_PRESETS } from '../settings/providerPresets'
import { applyProviderDraftIntent } from '../settings/providerDraftIntents'
import { makeProvider, makeSettings } from '../settings/tabs/testFixtures'
import { buildModelPairOptions } from '../settings/utils'

describe('video model catalog', () => {
  it('gives every preset model an explicit supported protocol and official sources', () => {
    expect(new Set(VIDEO_MODELS.map(m => m.id)).size).toBe(VIDEO_MODELS.length)
    for (const model of VIDEO_MODELS) {
      const info=resolveModelInfo(model.id)
      expect(info.capabilities?.videoGeneration).toBe(true)
      expect(info.capabilities?.videoInput).toBe(false)
      expect(info.videoProtocol).toBe(model.protocol)
      expect(VIDEO_PROTOCOLS[info.videoProtocol!].sources.length).toBeGreaterThan(0)
    }
    expect(PROVIDER_PRESETS.find(p => p.name === 'MiniMax Video (H3)')?.baseUrl).toBe('https://api.minimax.cn')
  })
  it('only supplements the exact official endpoint, never an unrelated gateway', () => {
    expect(suggestedVideoModels('https://api.x.ai/v1/')).toContain('grok-imagine-video-1.5')
    expect(suggestedVideoModels('https://gateway.example/v1')).toEqual([])
    expect(suggestedVideoModels('https://api.minimax.cn')).not.toContain('MiniMax-Hailuo-2.3')
  })
  it('does not guess protocol from substrings and preserves explicit overrides', () => {
    expect(resolveModelInfo('private-seedance-video').videoProtocol).toBeUndefined()
    expect(isVideoGenerationModel('private-video',{modelOverrides:{'private-video':{videoProtocol:'seedance'}}})).toBe(true)
    expect(resolveModelInfo('MiniMax-H3',{'MiniMax-H3':{capabilities:{videoGeneration:false}}}).capabilities?.videoGeneration).toBe(false)
  })
  it('keeps media out of generic chat options and clears removed media selections', () => {
    const provider=makeProvider({enabledModels:['gpt-4o','MiniMax-H3']})
    const settings=makeSettings({providers:[provider]})
    settings.defaultModels.videoGeneration={providerId:provider.id,model:'MiniMax-H3'}
    expect(buildModelPairOptions([provider]).map(m => m.label)).not.toContain(`${provider.name} - MiniMax-H3`)
    const removed=applyProviderDraftIntent(settings,{type:'remove-model',id:provider.id,model:'MiniMax-H3'})
    expect(removed.defaultModels.videoGeneration).toEqual({providerId:'',model:''})
    expect(applyProviderDraftIntent(settings,{type:'delete',id:provider.id}).defaultModels.videoGeneration).toEqual({providerId:'',model:''})
  })
})
