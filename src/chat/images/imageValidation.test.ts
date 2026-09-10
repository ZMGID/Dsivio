import { expect, it } from 'vitest'
import { imageConfigIssue } from './imageValidation'
import { emptyBrief } from './types'
const config = { providerId: 'p', model: 'image', protocol: 'openai', agentProviderId: '', agentModel: '' }
it('blocks missing image configuration and incompatible output before generation', () => {
  const brief = { ...emptyBrief('gen'), ratio: '1:1', resolution: '1k' }
  expect(imageConfigIssue(config, brief)).toBe('')
  expect(imageConfigIssue({ ...config, model: '' }, brief)).toContain('模型')
  expect(imageConfigIssue(config, { ...brief, ratio: '9:16' })).toContain('OpenAI')
  expect(imageConfigIssue({ ...config, protocol: 'grok' }, { ...brief, resolution: '4k' })).toContain('4K')
  expect(imageConfigIssue(config, { ...brief, count: 0 })).toContain('1–30')
})
