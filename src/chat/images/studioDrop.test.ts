import { describe, expect, it } from 'vitest'
import { dropAsProducts, looksLikeImagePath } from './studioDrop'

describe('studio drop paths', () => {
  it('treats image files as photos and anything else as a folder', () => {
    expect(looksLikeImagePath('C:\\goods\\front.png')).toBe(true)
    expect(looksLikeImagePath('/Users/me/sku/a.WEBP')).toBe(true)
    expect(looksLikeImagePath('C:\\goods\\sku-a')).toBe(false)
    expect(looksLikeImagePath('C:\\goods\\sku-a\\')).toBe(false)
  })
  it('imports folders as SKUs, and image-only drops as one product', () => {
    expect(dropAsProducts(['C:\\goods\\front.png', 'C:\\goods\\back.jpg'], 'replace')).toBe(false)
    expect(dropAsProducts(['C:\\goods\\sku-a'], 'replace')).toBe(true)
    expect(dropAsProducts(['C:\\goods\\front.png', 'C:\\goods\\sku-a'], 'replace')).toBe(true)
    expect(dropAsProducts(['C:\\goods\\front.png'], 'client')).toBe(true)
  })
})
