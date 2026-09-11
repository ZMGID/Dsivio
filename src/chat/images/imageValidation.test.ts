import { expect, it } from 'vitest'
import { humanizeImageError, imageConfigIssue } from './imageValidation'
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

it('does not dump gateway size or timeout payloads to the user', () => {
  expect(humanizeImageError('图片接口 HTTP 400 Bad Request：{"error":{"message":"image-2 size must be 16-multiple dimensions, long edge \\u003c= 3840, ratio \\u003c= 3:1, and 655360-8294400 pixels: 1:1"}}')).toContain('像素尺寸')
  expect(humanizeImageError('图片接口 HTTP 504 Gateway Timeout：error code: 504')).toContain('超时')
  expect(humanizeImageError('本地草稿保存失败')).toBe('本地草稿保存失败')
  expect(humanizeImageError(null)).toBe('')
})
