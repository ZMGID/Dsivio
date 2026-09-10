// @vitest-environment jsdom
import { beforeEach, expect, it } from 'vitest'
import { emptyBrief } from './types'
import { newImageDraftBrief, rememberImageSettings } from './draft'
beforeEach(() => localStorage.clear())
it('keeps output settings across new image tasks without copying product content', () => {
  rememberImageSettings({ ...emptyBrief('gen'), ratio: '16:9', resolution: '2k', requirement: 'old request' })
  expect(newImageDraftBrief('design')).toMatchObject({ feature: 'design', ratio: '16:9', resolution: '2k', requirement: '', products: [] })
})
