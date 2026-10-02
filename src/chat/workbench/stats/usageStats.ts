import type { UsageRange, UsageStatsResponse } from '../../../api/tauri'
import type { MediaTask } from '../../../generated/mediaGeneration'

/** Media generations counted on the usage page. Text records are not generations. */
export const MEDIA_USAGE_KINDS = new Set(['image', 'video', 'speech', 'transcribe', 'edit'])

export type UsageWindow = 7 | 30 | 90

export interface UsageDayRow {
  date: string
  label: string
  requests: number
  tokens: number
  media: number
}

export interface UsageModelRow {
  model: string
  requests: number
  tokens: number
  media: number
}

export function usageQueryRange(days: UsageWindow): UsageRange {
  if (days === 30) return '30d'
  if (days === 90) return '90d'
  return '7d'
}

function pad(value: number): string {
  return String(value).padStart(2, '0')
}

export function localDayKey(value: Date): string {
  return `${value.getFullYear()}-${pad(value.getMonth() + 1)}-${pad(value.getDate())}`
}

function inWindow(iso: string, days: UsageWindow, now: Date): boolean {
  const at = new Date(iso)
  if (Number.isNaN(at.getTime())) return false
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() - (days - 1))
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1)
  return at >= start && at < end
}

function dayLabel(date: string): string {
  const [, month, day] = date.split('-')
  return month && day ? `${month}/${day}` : date
}

/**
 * Token rows come from `usage_get_stats` (already limited to the selected range).
 * Media rows are counted here from `list_media_tasks`, excluding text records.
 */
export function aggregateUsage(
  stats: UsageStatsResponse,
  tasks: MediaTask[],
  days: UsageWindow,
  now = new Date(),
): { byDay: UsageDayRow[]; byModel: UsageModelRow[]; empty: boolean } {
  const mediaByDay = new Map<string, number>()
  const mediaByModel = new Map<string, number>()
  for (const task of tasks) {
    const kind = task.kind as string
    if (!MEDIA_USAGE_KINDS.has(kind) || kind === 'text') continue
    if (!inWindow(task.createdAt, days, now)) continue
    const key = localDayKey(new Date(task.createdAt))
    mediaByDay.set(key, (mediaByDay.get(key) ?? 0) + 1)
    const model = task.model || 'unknown'
    mediaByModel.set(model, (mediaByModel.get(model) ?? 0) + 1)
  }

  const byDay: UsageDayRow[] = stats.trend.map((point) => ({
    date: point.date,
    label: point.label,
    requests: point.requests,
    tokens: point.totalTokens,
    media: mediaByDay.get(point.date) ?? 0,
  }))
  for (const [date, media] of mediaByDay) {
    if (!byDay.some((row) => row.date === date)) {
      byDay.push({ date, label: dayLabel(date), requests: 0, tokens: 0, media })
    }
  }
  byDay.sort((a, b) => a.date.localeCompare(b.date))

  const byModel = new Map<string, UsageModelRow>()
  for (const group of stats.modelStats) {
    const model = group.model || group.label || group.id
    const row = byModel.get(model) ?? { model, requests: 0, tokens: 0, media: 0 }
    row.requests += group.requestCount
    row.tokens += group.totalTokens
    byModel.set(model, row)
  }
  for (const [model, media] of mediaByModel) {
    const row = byModel.get(model) ?? { model, requests: 0, tokens: 0, media: 0 }
    row.media += media
    byModel.set(model, row)
  }
  const modelRows = [...byModel.values()].sort((a, b) => b.tokens - a.tokens || b.media - a.media || a.model.localeCompare(b.model))
  const hasDay = byDay.some((row) => row.requests > 0 || row.tokens > 0 || row.media > 0)
  const hasModel = modelRows.some((row) => row.requests > 0 || row.tokens > 0 || row.media > 0)
  const empty = !hasDay && !hasModel
  return { byDay: empty ? [] : byDay, byModel: empty ? [] : modelRows, empty }
}
