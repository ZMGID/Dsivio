import { describe, expect, it } from 'vitest'
import type { UsageGroupStats, UsageStatsResponse, UsageTrendPoint } from '../../../api/tauri'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { aggregateUsage, usageQueryRange } from './usageStats'

const now = new Date(2026, 9, 2, 12, 0, 0)

function point(date: string, requests: number, tokens: number): UsageTrendPoint {
  return {
    date,
    label: date.slice(5).replace('-', '/'),
    requests,
    totalTokens: tokens,
    inputTokens: tokens,
    outputTokens: 0,
    cachedInputTokens: 0,
    cacheCreationInputTokens: 0,
    costUsd: 0,
  }
}

function model(name: string, requests: number, tokens: number): UsageGroupStats {
  return {
    id: `p::${name}`,
    label: name,
    model: name,
    requestCount: requests,
    successCount: requests,
    totalTokens: tokens,
    inputTokens: tokens,
    outputTokens: 0,
    cachedInputTokens: 0,
    cacheCreationInputTokens: 0,
    costUsd: 0,
  }
}

function stats(trend: UsageTrendPoint[] = [], modelStats: UsageGroupStats[] = []): UsageStatsResponse {
  return {
    summary: {
      totalRequests: 0,
      successfulRequests: 0,
      failedRequests: 0,
      missingUsageRequests: 0,
      providerReportedRequests: 0,
      totalTokens: 0,
      inputTokens: 0,
      outputTokens: 0,
      cachedInputTokens: 0,
      cacheCreationInputTokens: 0,
      reasoningTokens: 0,
      totalCostUsd: 0,
    },
    trend,
    logs: [],
    providerStats: [],
    modelStats,
    totalLogs: 0,
    skippedRecords: 0,
  }
}

function task(partial: Partial<MediaTask> & Pick<MediaTask, 'kind' | 'model' | 'createdAt'>): MediaTask {
  return {
    id: partial.id ?? partial.model,
    providerId: 'p',
    status: 'succeeded',
    error: null,
    remoteId: null,
    outputs: [],
    canResume: false,
    origin: null,
    prompt: '',
    result: null,
    requestHash: null,
    cancellation: null,
    ...partial,
  }
}

describe('aggregateUsage', () => {
  it('maps 7/30/90 onto the usage stats range', () => {
    expect(usageQueryRange(7)).toBe('7d')
    expect(usageQueryRange(30)).toBe('30d')
    expect(usageQueryRange(90)).toBe('90d')
  })

  it('counts tokens from usage stats and media generations, excluding text and older days', () => {
    const today = new Date(2026, 9, 2, 9).toISOString()
    const old = new Date(2026, 8, 1, 9).toISOString()
    const view = aggregateUsage(stats(
      [point('2026-10-02', 2, 30)],
      [model('gpt', 2, 30)],
    ), [
      task({ kind: 'image', model: 'gpt', createdAt: today }),
      task({ id: 'edit', kind: 'edit' as MediaTask['kind'], model: 'edit-1', createdAt: today }),
      task({ id: 'text', kind: 'text' as MediaTask['kind'], model: 'record', createdAt: today }),
      task({ id: 'old', kind: 'video', model: 'gpt', createdAt: old }),
    ], 7, now)
    expect(view.empty).toBe(false)
    expect(view.byDay.find((row) => row.date === '2026-10-02')).toMatchObject({ requests: 2, tokens: 30, media: 2 })
    expect(view.byModel.find((row) => row.model === 'gpt')).toMatchObject({ requests: 2, tokens: 30, media: 1 })
    expect(view.byModel.find((row) => row.model === 'edit-1')?.media).toBe(1)
    expect(view.byModel.find((row) => row.model === 'record')).toBeUndefined()
  })

  it('is empty when the range has only zero-filled days', () => {
    const view = aggregateUsage(stats([point('2026-10-02', 0, 0)]), [], 7, now)
    expect(view.empty).toBe(true)
    expect(view.byDay).toEqual([])
    expect(view.byModel).toEqual([])
  })
})
