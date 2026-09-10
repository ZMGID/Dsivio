import { newVideoBrief, type VideoBrief, type VideoTask, type VideoRoute } from './types'
export type VideoEntry = 'creation' | 'analysis' | 'remake'
export interface VideoDraft { brief: VideoBrief; task?: VideoTask; script: string; step: number; dirty: boolean }
const KEY = 'dsivio-video-drafts-v1'
const taskKey = (id: string) => `${KEY}:task:${id}`
const PREFERENCES_KEY = 'dsivio-video-preferences-v1'
type Preferences = { assistantId?: string; route?: VideoRoute; resolutions?: Partial<Record<VideoRoute, string>> }
function preferences(): Preferences {
  try { return JSON.parse(localStorage.getItem(PREFERENCES_KEY) || '{}') || {} } catch { return {} }
}
export function videoResolutions(b: VideoBrief): string[] {
  if (b.route === 'grok') {
    const reference = b.inputMode === 'reference' || ((!b.inputMode || b.inputMode === 'auto') && (b.images.length > 0 || !!b.voiceIds?.length))
    return reference ? ['480p', '720p'] : ['480p', '720p', '1080p']
  }
  return b.route === 'minimax' ? ['768P', '2K'] : b.route === 'comfy' ? ['0.5', '1'] : []
}
export function rememberVideoSettings(b: VideoBrief) {
  if (!b.route) return
  const previous = preferences()
  const resolutions = { ...previous.resolutions }
  if (videoResolutions(b).includes(b.resolution)) resolutions[b.route] = b.resolution
  try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ...previous, route: b.route, resolutions })) } catch { /* Draft still retains the settings. */ }
}
export function rememberVideoAssistant(assistantId: string) {
  try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ...preferences(), assistantId })) } catch { /* Keep the current draft usable. */ }
}
export function preferredVideoResolution(b: VideoBrief): string {
  const value = preferences().resolutions?.[b.route] || ''
  return videoResolutions(b).includes(value) ? value : ''
}
export function newVideoDraftBrief(mode: VideoBrief['mode'] = 'creation'): VideoBrief {
  const b = newVideoBrief(mode)
  const preferred = preferences()
  if (typeof preferred.assistantId === 'string') b.assistantId = preferred.assistantId
  const route = preferred.route
  if (route && ['grok', 'minimax', 'comfy'].includes(route)) {
    b.route = route
    b.resolution = preferredVideoResolution(b)
  }
  return b
}
export function readVideoTaskDraft(id: string): VideoDraft | undefined {
  try {
    const draft = JSON.parse(localStorage.getItem(taskKey(id)) || 'null') as VideoDraft | null
    return draft?.task?.id === id && draft.brief && typeof draft.script === 'string' ? draft : undefined
  } catch { return undefined }
}
export function removeVideoTaskDraft(id: string) {
  try { localStorage.removeItem(taskKey(id)) } catch { /* A deleted task cannot be reopened. */ }
}
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
  try {
    if (draft.task) {
      if (draft.dirty) localStorage.setItem(taskKey(draft.task.id), JSON.stringify(draft))
      else localStorage.removeItem(taskKey(draft.task.id))
    }
    localStorage.setItem(KEY, JSON.stringify({ ...readVideoDrafts(), [entry]: draft }))
    return true
  } catch { return false }
}
