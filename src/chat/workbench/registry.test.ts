import { describe, expect, it } from 'vitest'
import { WORKBENCH_FEATURES, WORKBENCH_GROUPS, isWorkbenchSubpage, workbenchFeature } from './registry'

describe('workbench registry', () => {
  it('keeps feature ids unique and every feature in a declared group', () => {
    const ids = WORKBENCH_FEATURES.map((feature) => feature.id)
    expect(new Set(ids).size).toBe(ids.length)
    const groups = new Set<string>(WORKBENCH_GROUPS.map((group) => group.id))
    for (const feature of WORKBENCH_FEATURES) expect(groups.has(feature.group)).toBe(true)
  })

  it('answers membership from the same table the sidebar reads', () => {
    expect(isWorkbenchSubpage('shops')).toBe(true)
    expect(isWorkbenchSubpage('home')).toBe(false)
    expect(isWorkbenchSubpage('chatgpt')).toBe(false)
    expect(workbenchFeature('main').group).toBe('image')
  })

  it('loads every registered page component', async () => {
    for (const feature of WORKBENCH_FEATURES) {
      const Page = await feature.load()
      expect(typeof Page, feature.id).toBe('function')
    }
  })
})
