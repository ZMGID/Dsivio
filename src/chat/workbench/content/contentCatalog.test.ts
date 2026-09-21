import { describe, expect, it } from 'vitest'
import { ASSET_TABS } from './contentCatalog'

describe('contentCatalog', () => {
  it('keeps the live asset tabs', () => {
    expect(ASSET_TABS.map((item) => item.id)).toEqual(['all', 'image', 'video'])
  })
})
