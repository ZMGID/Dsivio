// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from 'vitest'
import { newVideoBrief } from './types'
import { newVideoDraftBrief, rememberVideoAssistant, preferredVideoResolution, rememberVideoSettings, videoResolutions } from './videoDrafts'

beforeEach(() => localStorage.clear())
describe('video generation preferences', () => {
  it('remembers an assistant before service selection and keeps it through service changes', () => {
    rememberVideoAssistant('asst_builtin_video_ugc')
    expect(newVideoDraftBrief().assistantId).toBe('asst_builtin_video_ugc')
    rememberVideoSettings({ ...newVideoBrief(), route: 'grok', resolution: '720p' })
    expect(newVideoDraftBrief().assistantId).toBe('asst_builtin_video_ugc')
    rememberVideoAssistant('')
    expect(newVideoDraftBrief().assistantId).toBe('')
  })
  it('remembers service and per-service resolution without carrying content or first-frame mode', () => {
    rememberVideoSettings({ ...newVideoBrief(), route: 'grok', resolution: '720p', inputMode: 'image', images: ['old.png'], request: 'old story' })
    expect(newVideoDraftBrief()).toMatchObject({ route: 'grok', resolution: '720p', inputMode: 'auto', images: [], request: '' })
    rememberVideoSettings({ ...newVideoBrief(), route: 'minimax', resolution: '2K' })
    expect(newVideoDraftBrief()).toMatchObject({ route: 'minimax', resolution: '2K' })
    expect(preferredVideoResolution({ ...newVideoBrief(), route: 'grok' })).toBe('720p')
  })
  it('does not apply incompatible resolution or erase the last valid choice', () => {
    rememberVideoSettings({ ...newVideoBrief(), route: 'grok', resolution: '1080p' })
    const reference = { ...newVideoBrief(), route: 'grok' as const, images: ['product.png'], resolution: '1080p' }
    expect(videoResolutions(reference)).toEqual(['480p', '720p'])
    expect(preferredVideoResolution(reference)).toBe('')
    rememberVideoSettings({ ...reference, resolution: '' })
    expect(newVideoDraftBrief().resolution).toBe('1080p')
  })
  it('recovers from broken preferences without choosing a service', () => {
    localStorage.setItem('dsivio-video-preferences-v1', '{broken')
    expect(newVideoDraftBrief().route).toBe('')
  })
})
