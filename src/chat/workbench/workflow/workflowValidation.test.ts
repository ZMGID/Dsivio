import { describe, expect, it } from 'vitest'
import { makeProvider, makeSettings } from '../../../settings/tabs/testFixtures'
import { blankWorkflow } from './workflowModel'
import { modelProblems } from './workflowValidation'
import { imageOutputKey, listImageOutputs } from '../image/projects/imageOutput'

describe('workflow model capability validation', () => {
  const provider = makeProvider({ enabledModels: ['gpt-image-1', 'gpt-4o'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } }, 'gpt-4o': { capabilities: { vision: true } } } })
  const settings = makeSettings({ providers: [provider], workbenchMedia: { imageModels: [{ providerId: provider.id, model: 'gpt-image-1' }], videoModels: [] } })
  it('checks current pool membership and parameter definitions without executing', () => {
    const output = listImageOutputs('gpt-image-1')[0]
    const flow = { ...blankWorkflow('model'), nodes: [{ id: 'image', kind: 'image.generate' as const, title: '生成', position: { x: 0, y: 0 }, config: { type: 'generate' as const, model: { providerId: provider.id, model: 'gpt-image-1' }, prompt: 'hello', assets: [], options: { output: imageOutputKey(output.ratio, output.resolution) } } }] }
    expect(modelProblems(flow, settings)).toEqual([])
    flow.nodes[0].config.options.output = 'not-supported'
    expect(modelProblems(flow, settings).join()).toContain('尺寸参数')
    expect(modelProblems(flow, { ...settings, workbenchMedia: { imageModels: [], videoModels: [] } }).join()).toContain('模型池')
  })
})
