export const ASSISTANT_PLAZA_CATEGORIES = [
  'writing',
  'coding',
  'research',
  'workplace',
  'ecommerce',
  'video',
] as const

export const ASSISTANT_PROMPT_CATEGORIES = ['image', 'video', 'general'] as const
export type AssistantPromptCategory = (typeof ASSISTANT_PROMPT_CATEGORIES)[number]
export type AssistantPromptCategoryFilter = 'all' | 'favorite' | AssistantPromptCategory

export const ASSISTANT_PROMPT_CATEGORY_LABELS: Record<AssistantPromptCategory, string> = {
  image: '图片',
  video: '视频',
  general: '通用',
}

export function assistantPromptCategory(assistant: { category?: string }): AssistantPromptCategory {
  const value = (assistant.category ?? '').trim()
  if (value === 'image' || value === 'video' || value === 'general') return value
  return 'general'
}

export function assistantFitsPurpose(
  assistant: { category?: string },
  purpose: 'image_brief' | 'video_brief',
): boolean {
  const category = assistantPromptCategory(assistant)
  return category === 'general' || category === (purpose === 'image_brief' ? 'image' : 'video')
}

export type AssistantPlazaCategory = (typeof ASSISTANT_PLAZA_CATEGORIES)[number]
export type PlazaCategoryFilter = 'all' | AssistantPlazaCategory

export function assistantPlazaCategory(assistant: { category?: string }): AssistantPlazaCategory | '' {
  const value = (assistant.category ?? '').trim()
  return (ASSISTANT_PLAZA_CATEGORIES as readonly string[]).includes(value)
    ? (value as AssistantPlazaCategory)
    : ''
}

export function assistantMatchesPlazaCategory(
  assistant: { category?: string },
  filter: PlazaCategoryFilter,
): boolean {
  if (filter === 'all') return true
  return assistantPlazaCategory(assistant) === filter
}
