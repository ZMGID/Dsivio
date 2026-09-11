import { expect, it } from 'vitest'
import {
  imageOutputChoices,
  imageOutputKey,
  imageOutputLabel,
  imageOutputPixels,
  imageOutputRatios,
  imageRatioLabel,
  imageOutputResolutions,
  imageResolutionLabel,
  isAllowedImageOutput,
  listImageOutputs,
  parseImageOutputKey,
  snapImageOutput,
} from './imageOutput'

it('uses the official gpt-image-2 size table, not a homemade grid', () => {
  expect(imageOutputPixels('1:1', '1k', 'gpt-image-2')).toEqual([1024, 1024])
  expect(imageOutputPixels('1:1', '2k', 'gpt-image-2')).toEqual([2048, 2048])
  expect(imageOutputPixels('2:3', '1k', 'gpt-image-2')).toEqual([1024, 1536])
  expect(imageOutputPixels('3:2', '1k', 'gpt-image-2')).toEqual([1536, 1024])
  expect(imageOutputPixels('9:16', '1k', 'gpt-image-2')).toEqual([864, 1536])
  expect(imageOutputPixels('9:16', '2k', 'gpt-image-2')).toEqual([1152, 2048])
  expect(imageOutputPixels('9:16', '4k', 'gpt-image-2')).toEqual([2160, 3840])
  expect(imageOutputPixels('16:9', '1k', 'gpt-image-2')).toEqual([1536, 864])
  expect(imageOutputPixels('16:9', '2k', 'gpt-image-2')).toEqual([2048, 1152])
  expect(imageOutputPixels('16:9', '4k', 'gpt-image-2')).toEqual([3840, 2160])
  expect(isAllowedImageOutput('1:1', '4k', 'gpt-image-2', 'async')).toBe(false)
  expect(isAllowedImageOutput('4:3', '1k', 'gpt-image-2', 'async')).toBe(false)
  expect(isAllowedImageOutput('4:5', '2k', 'gpt-image-2', 'async')).toBe(false)
  expect(isAllowedImageOutput('2:3', '2k', 'gpt-image-2', 'async')).toBe(false)
  expect(imageOutputRatios('gpt-image-2', 'async')).toEqual(['1:1', '2:3', '3:2', '9:16', '16:9'])
  expect(imageOutputResolutions('gpt-image-2', 'async', '9:16').map(imageResolutionLabel)).toEqual([
    '1K · 864×1536',
    '2K · 1152×2048',
    '4K · 2160×3840',
  ])
  expect(imageOutputResolutions('gpt-image-2', 'async', '1:1').map(imageResolutionLabel)).toEqual([
    '1K · 1024×1024',
    '2K · 2048×2048',
  ])
  expect(snapImageOutput('gpt-image-2', 'async', '9:16', '1k')).toEqual({ ratio: '9:16', resolution: '1k' })
  expect(snapImageOutput('gpt-image-2', 'async', '1:1', '4k')).toEqual({ ratio: '1:1', resolution: '1k' })
  expect(imageRatioLabel('9:16')).toBe('9:16 竖版')
  expect(imageRatioLabel('16:9')).toBe('16:9 横版')
  expect(imageRatioLabel('1:1')).toBe('1:1 正方形')
  expect(imageOutputLabel({ ratio: '1:1', resolution: '1k', width: 1024, height: 1024 })).toBe('1024×1024 · 正方形')
  expect(parseImageOutputKey(imageOutputKey('16:9', '2k'))).toEqual({ ratio: '16:9', resolution: '2k' })
})

it('keeps earlier GPT Image and DALL·E models on their official enums', () => {
  expect(imageOutputPixels('2:3', '1k', 'gpt-image-1', 'openai')).toEqual([1024, 1536])
  expect(isAllowedImageOutput('9:16', '1k', 'gpt-image-1', 'openai')).toBe(false)
  expect(listImageOutputs('dall-e-3', 'openai').map((item) => `${item.width}x${item.height}`)).toEqual([
    '1024x1024',
    '1024x1792',
    '1792x1024',
  ])
})

it('lists Grok as ratio plus 1K/2K, not pixels', () => {
  const labels = listImageOutputs('grok-imagine-image', 'grok').map(imageOutputLabel)
  expect(labels).toContain('1K · 正方形')
  expect(labels).toContain('1K · 9:16')
  expect(labels).toContain('2K · 16:9')
  expect(labels.some((label) => label.includes('×'))).toBe(false)
  expect(isAllowedImageOutput('1:1', '4k', 'grok-imagine-image', 'grok')).toBe(false)
  expect(isAllowedImageOutput('4:5', '1k', 'grok-imagine-image', 'grok')).toBe(false)
})

it('still shows the current size when it is outside the current model', () => {
  const choices = imageOutputChoices('gpt-image-1', 'openai', '16:9', '2k')
  expect(choices.some((item) => item.ratio === '16:9' && item.resolution === '2k')).toBe(true)
  expect(choices.some((item) => item.ratio === '1:1' && item.resolution === '1k')).toBe(true)
})
