import { describe, expect, it } from 'vitest'
import { makeProvider } from '../../settings/tabs/testFixtures'
import { inferImageStudioProtocol, isImageGenerationModel, isVisionModel, reconcileImageStudioProtocol } from './studioModels'

describe('studioModels', () => {
  it('keeps chat-only providers out of the image-generation list', () => {
    const deepseek = makeProvider({
      id: 'deepseek',
      name: 'DeepSeek',
      enabledModels: ['deepseek-v4-flash', 'deepseek-v4-pro'],
    })
    expect(isImageGenerationModel(deepseek, 'deepseek-v4-flash')).toBe(false)
    expect(isImageGenerationModel(deepseek, 'deepseek-v4-pro')).toBe(false)
  })

  it('keeps catalog and adapter image models', () => {
    const openai = makeProvider({
      id: 'openai',
      enabledModels: ['gpt-image-1', 'gpt-4o'],
    })
    expect(isImageGenerationModel(openai, 'gpt-image-1')).toBe(true)
    expect(isImageGenerationModel(openai, 'gpt-4o')).toBe(false)
    expect(isVisionModel(openai, 'gpt-4o')).toBe(true)
  })

  it('infers studio protocol from the provider adapter instead of a user picker', () => {
    expect(
      inferImageStudioProtocol(makeProvider({ apiFormat: 'gemini' }), 'gemini-3.1-flash-image'),
    ).toBe('gemini')
    expect(
      inferImageStudioProtocol(
        makeProvider({ apiFormat: 'xai_responses', baseUrl: 'https://api.x.ai/v1' }),
        'grok-imagine-image',
      ),
    ).toBe('grok')
    expect(
      inferImageStudioProtocol(makeProvider({ apiFormat: 'openai_chat' }), 'gemini-3.1-flash-image'),
    ).toBe('gemini-chat')
    expect(inferImageStudioProtocol(makeProvider(), 'gpt-image-1')).toBe('openai')
    expect(inferImageStudioProtocol(makeProvider({ baseUrl: 'https://api.apimart.ai/v1' }), 'gpt-image-1')).toBe('async')
    expect(
      inferImageStudioProtocol(
        makeProvider({ baseUrl: 'https://ybw-ai.com/v1' }),
        'gpt-image-2',
      ),
    ).toBe('async')
  })

  it('upgrades a stale openai protocol for relay gpt-image models', () => {
    const relay = makeProvider({ baseUrl: 'https://ybw-ai.com/v1' })
    expect(
      reconcileImageStudioProtocol({ protocol: 'openai', model: 'gpt-image-2' }, relay).protocol,
    ).toBe('async')
    expect(
      reconcileImageStudioProtocol({ protocol: 'async', model: 'custom-image' }, relay).protocol,
    ).toBe('async')
    expect(
      reconcileImageStudioProtocol(
        { protocol: 'openai', model: 'gpt-image-2' },
        makeProvider(),
      ).protocol,
    ).toBe('openai')
  })
})
