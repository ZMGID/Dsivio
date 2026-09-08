export type VideoRoute = '' | 'comfy' | 'minimax' | 'grok'
export interface VideoBrief {
  name: string
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
}
export interface VideoTask {
  id: string
  revision: number
  updatedAt: number
  brief: VideoBrief
  script: string
  prompt: string
  approved: boolean
  status: string
  error?: string
  output?: string
  remote?: { route: VideoRoute; id?: string; base_url: string }
  quote?: {
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
  language: 'pt-BR',
  source: '',
})
export const videoStatus: Record<string, string> = {
  draft: '草稿',
  approved: '剧本已确认',
  submitting: '提交中 · 请勿重复生成',
  running: '生成中',
  succeeded: '已完成',
  failed: '生成失败',
  uncertain: '提交结果待核查',
}
