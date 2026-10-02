import { describe, expect, it } from 'vitest'
import { buildSubtitleRequest, parseEditPlan, serializeEditPlan } from './localEditContract'

describe('local edit contract', () => {
  it('normalizes a subtitle language and refuses a locale tag', () => {
    expect(buildSubtitleRequest({ video: '/tmp/a.mp4', language: ' ZH ', burn: false }).options).toEqual({
      video: '/tmp/a.mp4', language: 'zh', burn: false,
    })
    expect(() => buildSubtitleRequest({ video: '/tmp/a.mp4', language: 'zh-CN', burn: true })).toThrow(/语言/)
    expect(() => buildSubtitleRequest({ video: '  ', language: 'en', burn: false })).toThrow(/视频/)
  })

  it('round-trips an edit plan without extra fields', () => {
    const text = serializeEditPlan({
      clips: [{ source: '/tmp/a.mp4', start: 1, end: 3 }, { source: '/tmp/b.mp4' }],
      aspect: '9:16',
      fit: 'crop',
      resolution: '1080p',
      music: { path: '/tmp/bed.mp3', volume: 0.4, originalVolume: 1 },
      subtitles: { path: '/tmp/cues.srt' },
    })
    expect(parseEditPlan(text)).toEqual({
      clips: [{ source: '/tmp/a.mp4', start: 1, end: 3 }, { source: '/tmp/b.mp4' }],
      aspect: '9:16',
      fit: 'crop',
      resolution: '1080p',
      music: { path: '/tmp/bed.mp3', volume: 0.4, originalVolume: 1 },
      subtitles: { path: '/tmp/cues.srt' },
    })
    expect(text).not.toContain('notes')
    expect(parseEditPlan('{"clips":[]}')).toBeUndefined()
  })
})
