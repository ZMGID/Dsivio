export const ASSISTANT_PLAZA_CATEGORIES = [
  'writing',
  'coding',
  'research',
  'workplace',
  'ecommerce',
] as const

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
