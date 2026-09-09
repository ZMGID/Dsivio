export type VideoRoute = '' | 'comfy' | 'minimax' | 'grok'
export interface VideoBrief {
  name: string
  selectedConcept?: string
  mode: 'creation' | 'analysis'
  request: string
  images: string[]
  duration: number
  ratio: string
  route: VideoRoute
  resolution: string
  language: string
  source: string
  template?: VideoTemplate
  inputMode?: 'auto' | 'text' | 'image' | 'reference' | 'frames'
  speechMode?: 'auto' | 'dialogue' | 'ambient' | 'silent'
  dialogue?: string
  music?: string
  firstFrame?: string
  lastFrame?: string
  referenceVideos?: string[]
  referenceAudios?: string[]
  voiceIds?: string[]
}
export interface VideoTask {
  id: string
  revision: number
  updatedAt: number
  brief: VideoBrief
  script: string
  concepts?: string[]
  prompt: string
  approved: boolean
  status: string
  submission?: { state: 'rejected' | 'uncertain'; httpStatus?: number; reason: string; retryable: boolean }
  error?: string
  output?: string
  remote?: { route: VideoRoute; id?: string; base_url: string }
  quote?: {
    pricingStatus?: 'reference' | 'unknown'
    currency?: string
    estimated_cost?: Record<string, string>
    balance?: unknown
    note: string
    at: number
  }
}
export interface VideoTemplate {
  id: string
  name: string
  kind?: 'generation' | 'reference'
  script?: string
  spec?: { duration_seconds?: number; aspect_ratio?: string }
  shots?: { time: string; purpose: string; action: string; camera: string }[]
}
export interface VideoProvider {
  base_url?: string
  model?: string
  ready?: boolean
}
export interface VideoBootstrap {
  tasks: VideoTask[]
  templates: VideoTemplate[]
  config: Record<string, VideoProvider>
  root: string
  configPath: string
  dependencies: {
    python: string
    comfy: boolean
    node: boolean
    ffmpeg: boolean
    analyzer?: boolean
    bundled?: boolean
  }
}
export const newVideoBrief = (
  mode: VideoBrief['mode'] = 'creation',
): VideoBrief => ({
  name: '',
  mode,
  request: '',
  images: [],
  duration: 10,
  ratio: '9:16',
  route: '',
  resolution: '',
  language: 'zh-CN',
  source: '',
  inputMode: 'auto',
  speechMode: 'auto',
  dialogue: '',
  music: '',
  referenceVideos: [],
  referenceAudios: [],
  voiceIds: [],
})

export const videoRatios = {
  comfy: ['9:16', '16:9', '1:1', '2:3', '3:2', '3:4', '4:3', '21:9'],
  minimax: ['9:16', '16:9', '1:1', '3:4', '4:3', '21:9', 'adaptive'],
  grok: ['9:16', '16:9', '1:1', '3:4', '4:3', '3:2', '2:3'],
}
export const videoStatus: Record<string, string> = {
  draft: '草稿',
  approved: '剧本已确认',
  submitting: '提交中 · 请勿重复生成',
  running: '生成中',
  succeeded: '已完成',
  failed: '生成失败',
  uncertain: '未收到提交结果',
}

export const languages = [
  ['pt-BR', '葡萄牙语（巴西）'],
  ['en-US', '英语（美国）'],
  ['en-GB', '英语（英国）'],
  ['es', '西班牙语'],
  ['zh-CN', '中文（普通话）'],
  ['ja', '日语'],
  ['ko', '韩语'],
  ['fr', '法语'],
  ['de', '德语'],
  ['it', '意大利语'],
  ['ar', '阿拉伯语'],
  ['id', '印尼语'],
  ['th', '泰语'],
  ['vi', '越南语'],
  ['custom', '其他语言…'],
]
