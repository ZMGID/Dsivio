import { emptyBrief, FEATURES, type ImageBrief, type ImagePlan } from './types'

export const DRAFT_KEY = 'dsivio-image-draft-v1'
const PREFERENCES_KEY = 'dsivio-image-preferences-v1'
export function rememberImageSettings(brief: ImageBrief) {
  try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ratio: brief.ratio })) } catch { /* Draft retains the selection. */ }
}
export function newImageDraftBrief(feature: ImageBrief['feature']): ImageBrief {
  const brief = emptyBrief(feature)
  if (feature === 'gen') return brief
  try {
    const saved = JSON.parse(localStorage.getItem(PREFERENCES_KEY) || '{}')
    if (typeof saved.ratio === 'string' && saved.ratio !== 'auto') brief.ratio = saved.ratio
  } catch { /* Use defaults if local preferences are unavailable. */ }
  return brief
}
export type StudioDraft = {
  brief: ImageBrief
  taskId?: string
  revision?: number
  plans?: ImagePlan[] | null
  stage?: 'brief' | 'plan' | 'results'
  modified?: boolean
}

export function readStudioDraft(scope?: string): StudioDraft | null {
  try {
    const raw = localStorage.getItem(scope ? `${DRAFT_KEY}:${scope}` : DRAFT_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw)
    const brief = parsed.brief || parsed // Initial local drafts contained only the brief.
    if (!FEATURES.some((f) => f.id === brief.feature) || !Array.isArray(brief.products)) return null
    if (typeof brief.requirement !== 'string' || typeof brief.language !== 'string') return null
    return {
      brief: { ...emptyBrief(brief.feature), ...brief },
      taskId: parsed.taskId,
      revision: parsed.revision,
      plans: parsed.plans,
      stage: ['brief', 'plan', 'results'].includes(parsed.stage) ? parsed.stage : undefined,
      modified: parsed.modified === true,
    }
  } catch {
    return null
  }
}

export function storeStudioDraft(draft: StudioDraft): boolean {
  try {
    localStorage.setItem(DRAFT_KEY, JSON.stringify(draft))
    localStorage.setItem(`${DRAFT_KEY}:feature:${draft.brief.feature}`, JSON.stringify(draft))
    if (draft.taskId) localStorage.setItem(`${DRAFT_KEY}:task:${draft.taskId}`, JSON.stringify(draft))
    return true
  } catch {
    return false
  }
}
