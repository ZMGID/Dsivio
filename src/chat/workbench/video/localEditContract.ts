import type { MediaRequest } from '../../../generated/mediaGeneration'

/** Local ffmpeg models from media_generation. Kind is wire `"edit"`; generated MediaKind is refreshed by protocol:generate. */
export const LOCAL_MEDIA_PROVIDER = 'local'
export const FFMPEG_SUBTITLE_MODEL = 'ffmpeg-subtitle'
export const FFMPEG_EDIT_MODEL = 'ffmpeg-edit'
export const SUBTITLE_ORIGIN = 'workbench/subs'

export const EDIT_ASPECTS = ['9:16', '16:9', '1:1', '3:4', '4:3'] as const
export const EDIT_FITS = ['pad', 'crop'] as const
export const EDIT_RESOLUTIONS = ['720p', '1080p'] as const
export type EditAspect = (typeof EDIT_ASPECTS)[number]
export type EditFit = (typeof EDIT_FITS)[number]
export type EditResolution = (typeof EDIT_RESOLUTIONS)[number]

export const SUBTITLE_LANGUAGES: ReadonlyArray<readonly [string, string]> = [
  ['zh', '中文'],
  ['en', '英语'],
  ['ja', '日语'],
  ['ko', '韩语'],
  ['es', '西班牙语'],
  ['pt', '葡萄牙语'],
  ['fr', '法语'],
  ['de', '德语'],
  ['it', '意大利语'],
  ['id', '印尼语'],
  ['th', '泰语'],
  ['vi', '越南语'],
  ['ar', '阿拉伯语'],
  ['ru', '俄语'],
]

export interface EditClip {
  source: string
  start?: number
  end?: number
}

export interface EditMusic {
  path: string
  volume?: number
  originalVolume?: number
}

export interface EditPlan {
  clips: EditClip[]
  aspect?: EditAspect
  fit?: EditFit
  resolution?: EditResolution
  music?: EditMusic
  subtitles?: { path: string }
}

export interface SubtitleRequestInput {
  video: string
  language: string
  burn: boolean
}

function finite(value: number | undefined): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

export function parseEditPlan(text: string): EditPlan | undefined {
  try {
    const value = JSON.parse(text) as EditPlan
    if (!value || typeof value !== 'object' || !Array.isArray(value.clips) || value.clips.length === 0) return undefined
    if (!value.clips.every(clip => clip && typeof clip.source === 'string' && clip.source.trim().length > 0)) return undefined
    return value
  } catch {
    return undefined
  }
}

export function serializeEditPlan(plan: EditPlan): string {
  const body: EditPlan = {
    clips: plan.clips.map(clip => {
      const item: EditClip = { source: clip.source }
      if (finite(clip.start)) item.start = clip.start
      if (finite(clip.end)) item.end = clip.end
      return item
    }),
  }
  if (plan.aspect && (EDIT_ASPECTS as readonly string[]).includes(plan.aspect)) body.aspect = plan.aspect
  if (plan.fit && (EDIT_FITS as readonly string[]).includes(plan.fit)) body.fit = plan.fit
  if (plan.resolution && (EDIT_RESOLUTIONS as readonly string[]).includes(plan.resolution)) body.resolution = plan.resolution
  if (plan.music?.path) {
    body.music = { path: plan.music.path }
    if (finite(plan.music.volume)) body.music.volume = plan.music.volume
    if (finite(plan.music.originalVolume)) body.music.originalVolume = plan.music.originalVolume
  }
  if (plan.subtitles?.path) body.subtitles = { path: plan.subtitles.path }
  return JSON.stringify(body, null, 2)
}

export function buildSubtitleRequest(input: SubtitleRequestInput): MediaRequest {
  const video = input.video.trim()
  const language = input.language.trim().toLowerCase()
  if (!video) throw new Error('请选择视频')
  if (!/^[a-z]{2,3}$/.test(language) || language === 'und') throw new Error('请选择 2–3 位小写语言代码')
  const request = {
    providerId: LOCAL_MEDIA_PROVIDER,
    model: FFMPEG_SUBTITLE_MODEL,
    kind: 'edit',
    prompt: '',
    images: [] as string[],
    options: { video, language, burn: input.burn },
    origin: SUBTITLE_ORIGIN,
  }
  return request as unknown as MediaRequest
}
