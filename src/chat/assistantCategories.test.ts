import { describe, expect, it } from 'vitest'
import {
  ASSISTANT_PLAZA_CATEGORIES,
  assistantMatchesPlazaCategory,
  assistantPlazaCategory,
} from './assistantCategories'

describe('assistant plaza categories', () => {
  it('accepts known plaza categories and drops unknown ones', () => {
    expect(assistantPlazaCategory({ category: 'ecommerce' })).toBe('ecommerce')
    expect(assistantPlazaCategory({ category: ' writing ' })).toBe('writing')
    expect(assistantPlazaCategory({ category: 'other' })).toBe('')
    expect(assistantPlazaCategory({})).toBe('')
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
