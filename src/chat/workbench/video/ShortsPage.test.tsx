import { expect, it } from 'vitest'
import { WORKBENCH_FEATURES } from '../registry'
it('loads independent video feature pages from the shared registry', async () => {
 const ids = ['shorts', 'vclone', 'video-analysis']
 const pages = await Promise.all(ids.map(id => WORKBENCH_FEATURES.find(f => f.id === id)!.load()))
 expect(new Set(pages).size).toBe(3)
 expect(WORKBENCH_FEATURES.filter(f => ids.includes(f.id)).every(f => f.group === 'video')).toBe(true)
})
