import { describe, expect, it } from 'vitest'
import {
  ASSISTANT_PLAZA_CATEGORIES,
  assistantMatchesPlazaCategory,
  assistantPlazaCategory,
  assistantFitsPurpose,
  assistantPromptCategory,
} from './assistantCategories'

describe('assistant plaza categories', () => {
  it('accepts known plaza categories and drops unknown ones', () => {
    expect(assistantPlazaCategory({ category: 'video' })).toBe('video')
    expect(assistantPlazaCategory({ category: 'ecommerce' })).toBe('ecommerce')
    expect(assistantPlazaCategory({ category: ' writing ' })).toBe('writing')
    expect(assistantPlazaCategory({ category: 'other' })).toBe('')
    expect(assistantPlazaCategory({})).toBe('')
  })

  it('maps prompt assistants to image, video, and general use', () => {
    expect(assistantPromptCategory({ category: 'image' })).toBe('image')
    expect(assistantPromptCategory({ category: 'video' })).toBe('video')
    expect(assistantPromptCategory({ category: 'writing' })).toBe('general')
    expect(assistantFitsPurpose({ category: 'image' }, 'image_brief')).toBe(true)
    expect(assistantFitsPurpose({ category: 'image' }, 'video_brief')).toBe(false)
    expect(assistantFitsPurpose({}, 'video_brief')).toBe(true)
  })

  it('filters plaza cards by category without hiding the all view', () => {
    const ecom = { category: 'ecommerce' }
    const writer = { category: 'writing' }
    expect(assistantMatchesPlazaCategory(ecom, 'all')).toBe(true)
    expect(assistantMatchesPlazaCategory(ecom, 'ecommerce')).toBe(true)
    expect(assistantMatchesPlazaCategory(writer, 'ecommerce')).toBe(false)
    expect(ASSISTANT_PLAZA_CATEGORIES).toContain('ecommerce')
  })
})
