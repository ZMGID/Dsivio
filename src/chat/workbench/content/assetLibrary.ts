import type { MediaTask } from '../../../generated/mediaGeneration'

/** Completed outputs the library and picker can show. Matches `MediaKind`. */
export const ASSET_KINDS = ['image', 'video', 'edit', 'speech', 'transcribe', 'text'] as const

export type AssetKind = (typeof ASSET_KINDS)[number]

export type AssetQuery = {
  kind: 'all' | AssetKind
  /** Empty lists every origin. */
  origin: string
  keyword: string
}

export function assetTitle(task: MediaTask): string {
  const result = task.result
  if (result && typeof result === 'object' && 'title' in result) {
    const title = (result as { title?: unknown }).title
    if (typeof title === 'string' && title.trim()) return title.trim()
  }
  const file = task.outputs[0]?.path.split(/[/\\]/).pop()
  return file || task.prompt.trim() || task.model
}

export function assetOrigins(tasks: MediaTask[]): string[] {
  const origins = new Set<string>()
  for (const task of tasks) {
    if (task.status === 'succeeded' && task.outputs.length > 0 && task.origin) origins.add(task.origin)
  }
  return [...origins].sort()
}

/** Succeeded tasks that actually have a file, narrowed by kind, origin, and keyword. */
export function filterAssets(tasks: MediaTask[], query: AssetQuery): MediaTask[] {
  const keyword = query.keyword.trim().toLowerCase()
  return tasks.filter((task) => {
    if (task.status !== 'succeeded' || task.outputs.length === 0) return false
    if (query.kind !== 'all' && task.kind !== query.kind) return false
    if (query.origin && task.origin !== query.origin) return false
    if (!keyword) return true
    const haystack = [
      assetTitle(task),
      task.prompt,
      task.origin ?? '',
      task.model,
      task.kind,
      ...task.outputs.map((output) => `${output.path} ${output.mime}`),
    ].join('\n').toLowerCase()
    return haystack.includes(keyword)
  })
}
