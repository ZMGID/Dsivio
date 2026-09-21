import { describe, expect, it } from 'vitest'
import { sizeForImageRatio } from './imageCatalog'

describe('imageCatalog', () => {
  it('keeps the live 1:1 and 3:4 sizes', () => {
    expect(sizeForImageRatio('1:1')).toBe('2048×2048')
    expect(sizeForImageRatio('3:4')).toBe('1920×2560')
  })
})
