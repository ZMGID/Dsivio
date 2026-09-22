import { act, renderHook } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { useAiTask } from './useAiTask'

vi.mock('../../api/tauri', () => ({ api: { runAiTask: vi.fn(), cancelAiTask: vi.fn() } }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))

beforeEach(() => {
  vi.mocked(api.runAiTask).mockReset()
  vi.mocked(api.cancelAiTask).mockReset()
})

it('reports an error and stays idle after a rejected call', async () => {
  vi.mocked(api.runAiTask).mockRejectedValue(new Error('no key'))
  const { result } = renderHook(() => useAiTask())
  let text: string | null = 'pending'
  await act(async () => {
    text = await result.current.run({ mode: 'once', prompt: 'hi', system: null, images: [], tools: [], slot: 'chat', providerId: null, model: null, cwd: null, timeoutSecs: null, stream: false })
  })
  expect(text).toBeNull()
  expect(result.current.busy).toBe(false)
  expect(result.current.error).toBe('no key')
})

it('drops a result that arrives after cancel', async () => {
  let resolveRun: (value: { text: string; toolCalls: []; usage: null }) => void = () => {}
  vi.mocked(api.runAiTask).mockImplementation(() => new Promise((resolve) => { resolveRun = resolve }))
  const { result } = renderHook(() => useAiTask())
  let returned: Promise<string | null> = Promise.resolve('pending')
  act(() => {
    returned = result.current.run({ mode: 'once', prompt: 'hi', system: null, images: [], tools: [], slot: 'chat', providerId: null, model: null, cwd: null, timeoutSecs: null, stream: false })
  })
  expect(result.current.busy).toBe(true)
  act(() => { result.current.cancel() })
  await act(async () => {
    resolveRun({ text: 'late', toolCalls: [], usage: null })
    expect(await returned).toBeNull()
  })
  expect(result.current.busy).toBe(false)
  expect(result.current.error).toBeNull()
  expect(api.cancelAiTask).toHaveBeenCalledTimes(1)
})


it('rejects concurrent calls without losing the active request or its cancellation', async () => {
  let resolveRun!: (value: { text: string; toolCalls: []; usage: null }) => void
  vi.mocked(api.runAiTask).mockImplementation(() => new Promise((resolve) => { resolveRun = resolve }))
  const { result } = renderHook(() => useAiTask())
  let first!: Promise<string | null>
  let duplicate!: Promise<string | null>
  act(() => {
    first = result.current.run({ mode: 'once', prompt: 'first' })
    duplicate = result.current.run({ mode: 'once', prompt: 'duplicate' })
  })
  expect(api.runAiTask).toHaveBeenCalledTimes(1)
  expect(await duplicate).toBeNull()
  expect(result.current.busy).toBe(true)
  const taskId = vi.mocked(api.runAiTask).mock.calls[0][0].taskId
  act(() => { result.current.cancel() })
  expect(api.cancelAiTask).toHaveBeenCalledTimes(1)
  expect(api.cancelAiTask).toHaveBeenCalledWith(taskId)
  await act(async () => {
    resolveRun({ text: 'late', toolCalls: [], usage: null })
    expect(await first).toBeNull()
  })
  expect(result.current.busy).toBe(false)
})

it('accepts a new call after success and cancels an active call on unmount', async () => {
  vi.mocked(api.runAiTask).mockResolvedValueOnce({ text: 'done', toolCalls: [], usage: null })
  const { result, unmount } = renderHook(() => useAiTask())
  await act(async () => { expect(await result.current.run({ mode: 'once', prompt: 'first' })).toBe('done') })
  expect(result.current.busy).toBe(false)
  let resolveRun!: (value: { text: string; toolCalls: []; usage: null }) => void
  vi.mocked(api.runAiTask).mockImplementationOnce(() => new Promise((resolve) => { resolveRun = resolve }))
  let second!: Promise<string | null>
  act(() => { second = result.current.run({ mode: 'once', prompt: 'second' }) })
  const taskId = vi.mocked(api.runAiTask).mock.calls[1][0].taskId
  unmount()
  expect(api.cancelAiTask).toHaveBeenCalledTimes(1)
  expect(api.cancelAiTask).toHaveBeenCalledWith(taskId)
  await act(async () => {
    resolveRun({ text: 'late', toolCalls: [], usage: null })
    expect(await second).toBeNull()
  })
})

it('locks input preparation immediately and drops it after cancel without disturbing a new call', async () => {
  let finishPreparing!: () => void
  const prepare = vi.fn(async () => {
    await new Promise<void>((resolve) => { finishPreparing = resolve })
    return { mode: 'once' as const, prompt: 'prepared' }
  })
  let resolveRun!: (value: { text: string; toolCalls: []; usage: null }) => void
  vi.mocked(api.runAiTask).mockImplementation(() => new Promise((resolve) => { resolveRun = resolve }))
  const { result } = renderHook(() => useAiTask())
  let first!: Promise<string | null>
  let duplicate!: Promise<string | null>
  act(() => {
    first = result.current.run(prepare)
    duplicate = result.current.run(prepare)
  })
  expect(prepare).toHaveBeenCalledTimes(1)
  expect(await duplicate).toBeNull()
  expect(result.current.busy).toBe(true)
  expect(api.runAiTask).not.toHaveBeenCalled()
  act(() => { result.current.cancel() })
  let next!: Promise<string | null>
  act(() => { next = result.current.run({ mode: 'once', prompt: 'next' }) })
  await act(async () => {
    finishPreparing()
    expect(await first).toBeNull()
  })
  expect(api.runAiTask).toHaveBeenCalledTimes(1)
  expect(result.current.busy).toBe(true)
  await act(async () => {
    resolveRun({ text: 'new result', toolCalls: [], usage: null })
    expect(await next).toBe('new result')
  })
  expect(result.current.busy).toBe(false)
})
