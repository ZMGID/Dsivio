import { describe, expect, it } from 'vitest'
import { newVideoBrief, type VideoBrief } from './types'
import { videoInputIssue, videoSetupIssue } from './videoValidation'
const valid = (): VideoBrief => ({ ...newVideoBrief(), request: '展示背包', route: 'grok', resolution: '720p' })
describe('video step prerequisites', () => {
  it.each([
    [{ route: '' }, '生成服务'], [{ resolution: '' }, '清晰度'],
    [{ duration: 0 }, '时长'], [{ duration: 2.5 }, '整数'], [{ duration: NaN }, '整数'],
    [{ ratio: '21:9' }, '画幅'], [{ request: '' }, '拍摄要求'],
    [{ images: ['a', 'b'], resolution: '1080p' }, '720p'],
    [{ images: ['a'], resolution: '1080p' }, '720p'],
    [{ inputMode: 'image', images: [] }, '恰好一张'],
    [{ referenceVideos: ['v.mp4'] }, 'MiniMax'],
    [{ voiceIds: ['v'], speechMode: 'silent' }, '非静音'],
    [{ route: 'minimax', resolution: '720p' }, '清晰度'],
  ] as [Partial<VideoBrief>, string][])('blocks invalid input %j', (patch, reason) => {
    expect(videoInputIssue({ ...valid(), ...patch })).toContain(reason)
  })
  it('allows supported generation, and analysis without a generation provider', () => {
    expect(videoInputIssue(valid())).toBe('')
    expect(videoInputIssue({ ...valid(), images: ['a'] })).toBe('')
    expect(videoInputIssue({ ...valid(), images: ['a'], inputMode: 'image', resolution: '1080p' })).toBe('')
    expect(videoInputIssue({ ...valid(), route: 'minimax', resolution: '2K' })).toBe('')
    expect(videoInputIssue({ ...newVideoBrief('analysis'), source: '/reference.mp4' })).toBe('')
    expect(videoInputIssue(newVideoBrief('analysis'))).toContain('参考视频')
    expect(videoInputIssue({ ...newVideoBrief('analysis'), source: '/reference.mp4' }, true, true)).toContain('商品图片')
    expect(videoInputIssue({ ...valid(), request: '' }, false, false, '已有剧本')).toBe('')
  })
  it('blocks an unconfigured provider before planning', () => {
    expect(videoSetupIssue(valid(), { tasks: [], templates: [], config: { grok: { ready: false } }, root: '', configPath: '', dependencies: { python: '', node: true, comfy: true, ffmpeg: true } })).toContain('API Key')
  })
})
