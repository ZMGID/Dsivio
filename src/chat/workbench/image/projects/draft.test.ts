// @vitest-environment jsdom
import { beforeEach, expect, it } from 'vitest'
import { emptyBrief } from './types'
import { newImageDraftBrief, rememberImageSettings } from './draft'
beforeEach(() => localStorage.clear())
it('keeps ratio but resets resolution to 1k across new image tasks', () => {
  rememberImageSettings({ ...emptyBrief('gen'), ratio: '16:9', resolution: '2k', requirement: 'old request' })
  expect(newImageDraftBrief('design')).toMatchObject({ feature: 'design', ratio: '16:9', resolution: '1k', requirement: '', products: [] })
})

it('starts quick image requests at 1k even after manual output preferences', () => {
  rememberImageSettings({ ...emptyBrief('gen'), ratio: '16:9', resolution: '2k' })
  expect(newImageDraftBrief('gen')).toMatchObject({ count: 0, ratio: 'auto', resolution: '1k', language: 'auto' })
})

it('ignores a legacy saved resolution preference', () => {
  localStorage.setItem('dsivio-image-preferences-v1', JSON.stringify({ ratio: '4:5', resolution: '4k' }))
  expect(newImageDraftBrief('smart')).toMatchObject({ ratio: '4:5', resolution: '1k' })
})
