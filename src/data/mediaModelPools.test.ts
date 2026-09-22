import { describe, expect, it } from 'vitest'
import { mediaPoolEntries } from './mediaModelPools'
import { makeProvider, makeSettings } from '../settings/tabs/testFixtures'
import { applyProviderDraftIntent } from '../settings/providerDraftIntents'

describe('media pool boundary', () => {
  it('does not inherit conversation defaults or select every enabled model', () => {
    const settings=makeSettings({providers:[makeProvider({enabledModels:['MiniMax-H3']})]})
    settings.defaultModels.videoGeneration={providerId:'p1',model:'MiniMax-H3'}
    expect(mediaPoolEntries(settings,'videoModels')).toEqual([])
  })
  it('preserves disabled membership, makes it unavailable, and removes deleted references', () => {
    const settings=makeSettings({providers:[makeProvider({enabled:false,enabledModels:['MiniMax-H3','grok-imagine-video']})],workbenchMedia:{imageModels:[],videoModels:[{providerId:'p1',model:'MiniMax-H3'},{providerId:'p1',model:'grok-imagine-video'}]}})
    expect(mediaPoolEntries(settings,'videoModels').every(m=>!m.available)).toBe(true)
    const removed=applyProviderDraftIntent(settings,{type:'remove-model',id:'p1',model:'MiniMax-H3'})
    expect(removed.workbenchMedia.videoModels).toEqual([{providerId:'p1',model:'grok-imagine-video'}])
    expect(applyProviderDraftIntent(settings,{type:'delete',id:'p1'}).workbenchMedia.videoModels).toEqual([])
  })
})
