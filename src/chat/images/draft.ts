import { emptyBrief, FEATURES, type ImageBrief, type ImagePlan } from './types'

export const DRAFT_KEY = 'dsivio-image-draft-v1'
const PREFERENCES_KEY = 'dsivio-image-preferences-v1'
export function rememberImageSettings(brief: ImageBrief) {
  try { localStorage.setItem(PREFERENCES_KEY, JSON.stringify({ ratio: brief.ratio, resolution: brief.resolution })) } catch { /* Draft retains the selections. */ }
}
export function newImageDraftBrief(feature: ImageBrief['feature']): ImageBrief {
  const brief = emptyBrief(feature)
  try {
    const saved = JSON.parse(localStorage.getItem(PREFERENCES_KEY) || '{}')
    if (typeof saved.ratio === 'string') brief.ratio = saved.ratio
    if (typeof saved.resolution === 'string') brief.resolution = saved.resolution
  } catch { /* Use defaults if local preferences are unavailable. */ }
  return brief
}
export type StudioDraft = {
  brief: ImageBrief
  taskId?: string
  revision?: number
  plans?: ImagePlan[] | null
}

export function readStudioDraft(): StudioDraft | null {
  try {
    const raw = localStorage.getItem(DRAFT_KEY)
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
    }
  } catch {
    return null
  }
}

export function storeStudioDraft(draft: StudioDraft): boolean {
  try {
    localStorage.setItem(DRAFT_KEY, JSON.stringify(draft))
    return true
  } catch {
    return false
  }
}
