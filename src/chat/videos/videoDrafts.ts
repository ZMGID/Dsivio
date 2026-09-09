import type { VideoBrief, VideoTask } from './types'
export type VideoEntry = 'creation' | 'analysis' | 'remake'
export interface VideoDraft { brief: VideoBrief; task?: VideoTask; script: string; step: number; dirty: boolean }
const KEY = 'dsivio-video-drafts-v1'
export function readVideoDrafts(): Partial<Record<VideoEntry, VideoDraft>> {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) || '{}')
    return Object.fromEntries(Object.entries(value).filter(([key, draft]) => {
      const d = draft as VideoDraft
      return ['creation', 'analysis', 'remake'].includes(key) && d?.brief && Array.isArray(d.brief.images) && typeof d.script === 'string' && typeof d.brief.request === 'string'
    }))
  } catch { return {} }
}
export function writeVideoDraft(entry: VideoEntry, draft: VideoDraft) {
  try { localStorage.setItem(KEY, JSON.stringify({ ...readVideoDrafts(), [entry]: draft })); return true } catch { return false }
}
