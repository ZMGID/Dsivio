import { emptyBrief, FEATURES, type ImageBrief, type ImagePlan } from './types'

export const DRAFT_KEY = 'dsivio-image-draft-v1'
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

export function storeStudioDraft(draft: StudioDraft): void {
  try {
    localStorage.setItem(DRAFT_KEY, JSON.stringify(draft))
  } catch {
    /* Explicit native Save remains available. */
  }
}
