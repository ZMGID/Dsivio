import { describe, expect, it } from 'vitest'
import { sizeForRatio } from './copyCatalog'

describe('copyCatalog', () => {
  it('keeps the live 3:4 size and derives the rest from the long edge', () => {
    expect(sizeForRatio('3:4')).toBe('1920x2560')
    expect(sizeForRatio('1:1')).toBe('1920x1920')
    expect(sizeForRatio('16:9')).toBe('2560x1440')
  })
})
