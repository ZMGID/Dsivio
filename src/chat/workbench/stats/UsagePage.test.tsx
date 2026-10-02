import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api, type UsageStatsResponse } from '../../../api/tauri'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { UsagePage } from './UsagePage'

vi.mock('../../../api/tauri', () => ({
  api: { usageGetStats: vi.fn(), listMediaTasks: vi.fn() },
}))

const empty: UsageStatsResponse = {
  summary: {
    totalRequests: 0, successfulRequests: 0, failedRequests: 0, missingUsageRequests: 0,
    providerReportedRequests: 0, totalTokens: 0, inputTokens: 0, outputTokens: 0,
    cachedInputTokens: 0, cacheCreationInputTokens: 0, reasoningTokens: 0, totalCostUsd: 0,
  },
  trend: [],
  logs: [],
  providerStats: [],
  modelStats: [],
  totalLogs: 0,
  skippedRecords: 0,
}

const filled: UsageStatsResponse = {
  ...empty,
  trend: [{
    date: '2026-10-02', label: '10/02', requests: 2, totalTokens: 40,
    inputTokens: 10, outputTokens: 30, cachedInputTokens: 0, cacheCreationInputTokens: 0, costUsd: 0,
  }],
  modelStats: [{
    id: 'p::gpt', label: 'gpt', model: 'gpt', requestCount: 2, successCount: 2, totalTokens: 40,
    inputTokens: 10, outputTokens: 30, cachedInputTokens: 0, cacheCreationInputTokens: 0, costUsd: 0,
  }],
}

const image: MediaTask = {
  id: 'img', providerId: 'p', model: 'gpt', kind: 'image', status: 'succeeded',
  createdAt: new Date().toISOString(), error: null, remoteId: null, outputs: [], canResume: false,
  origin: null, prompt: '', result: null, requestHash: null, cancellation: null,
}

beforeEach(() => {
  vi.mocked(api.usageGetStats).mockReset()
  vi.mocked(api.listMediaTasks).mockReset()
  vi.mocked(api.usageGetStats).mockResolvedValue(empty)
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
})

it('shows an empty state when the range has no calls or media', async () => {
  render(<UsagePage />)
  expect(await screen.findByText('还没有使用记录')).toBeInTheDocument()
  expect(api.usageGetStats).toHaveBeenCalledWith({ range: '7d', limit: 1 })
  expect(api.listMediaTasks).toHaveBeenCalledWith({})
})

it('lists token usage by day and model, and refetches when the range changes', async () => {
  vi.mocked(api.usageGetStats).mockResolvedValue(filled)
  vi.mocked(api.listMediaTasks).mockResolvedValue([image])
  render(<UsagePage />)
  expect(await screen.findByText('gpt')).toBeInTheDocument()
  expect(screen.getByText('10/02')).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '时间范围' }))
  fireEvent.click(screen.getByRole('option', { name: '30 天' }))
  await waitFor(() => expect(api.usageGetStats).toHaveBeenCalledWith({ range: '30d', limit: 1 }))
})

it('retries after a failed load', async () => {
  vi.mocked(api.usageGetStats).mockRejectedValueOnce(new Error('用量服务不可用'))
  render(<UsagePage />)
  expect(await screen.findByText(/用量服务不可用/)).toBeInTheDocument()
  vi.mocked(api.usageGetStats).mockResolvedValue(empty)
  fireEvent.click(screen.getByRole('button', { name: '重试' }))
  expect(await screen.findByText('还没有使用记录')).toBeInTheDocument()
})

it('drops a late stats response after leaving and loads again on reopen', async () => {
  let resolveStats: (value: UsageStatsResponse) => void = () => {}
  vi.mocked(api.usageGetStats).mockImplementationOnce(() => new Promise((resolve) => { resolveStats = resolve }))
  const first = render(<UsagePage />)
  await waitFor(() => expect(api.usageGetStats).toHaveBeenCalledTimes(1))
  first.unmount()
  await act(async () => { resolveStats(filled) })
  vi.mocked(api.usageGetStats).mockResolvedValue(empty)
  render(<UsagePage />)
  expect(await screen.findByText('还没有使用记录')).toBeInTheDocument()
  expect(screen.queryByText('gpt')).toBeNull()
  expect(api.usageGetStats).toHaveBeenCalledTimes(2)
  cleanup()
})
