import { act, renderHook } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { newVideoBrief, type VideoTask } from './types'
import { useVideoTaskProgress } from './useVideoTaskProgress'

vi.mock('../../api/tauri', () => ({ api: { videoStudioTask: vi.fn() } }))
afterEach(() => { vi.useRealTimers(); vi.clearAllMocks() })
it('rotates bounded progress checks across running tasks without submitting or retrying uncertain ones', async () => {
  vi.useFakeTimers()
  const tasks: VideoTask[] = Array.from({ length: 7 }, (_, i) => ({ id: `v${i}`, revision: 1, updatedAt: 0, brief: newVideoBrief(), script: '', prompt: '', approved: true, status: i === 6 ? 'uncertain' : 'running', remote: { route: 'grok', id: `remote${i}`, base_url: 'https://example.invalid' } }))
  vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => ({ ...tasks.find(t => t.id === input.id)!, revision: action === 'get' ? 2 : 3 }))
  const onUpdate = vi.fn()
  const hook = renderHook(() => useVideoTaskProgress(tasks, true, onUpdate))
  await act(async () => { await vi.advanceTimersByTimeAsync(1000) })
  expect(onUpdate).toHaveBeenCalledTimes(3)
  await act(async () => { await vi.advanceTimersByTimeAsync(8000) })
  expect(onUpdate).toHaveBeenCalledTimes(6)
  const calls = vi.mocked(api.videoStudioTask).mock.calls
  expect(new Set(calls.map(([, input]) => input.id)).size).toBe(6)
  expect(calls.every(([action]) => ['get', 'poll'].includes(action))).toBe(true)
  expect(calls.filter(([action]) => action === 'poll').every(([, input]) => input.revision === 2)).toBe(true)
  hook.unmount()
  await act(async () => { await vi.advanceTimersByTimeAsync(20000) })
  expect(onUpdate).toHaveBeenCalledTimes(6)
})
