import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ScheduledTask, ScheduledTaskRun, ScheduledTasksChangedEvent } from '../../api/scheduledTaskContracts'
import type { useScheduledTasks as ScheduledTasksHook } from './useScheduledTasks'

const api = vi.hoisted(() => ({
  scheduledTasksList: vi.fn(),
  scheduledTaskRuns: vi.fn(),
  onScheduledTasksChanged: vi.fn(),
}))

vi.mock('../../api/tauri', () => ({ api }))

const listeners = new Set<(event: ScheduledTasksChangedEvent) => void>()

function task(id = 'task'): ScheduledTask {
  return {
    id, name: id, prompt: 'prompt', schedule: { kind: 'daily', hour: 9, minute: 0 },
    conversationId: 'conversation', enabled: true, status: 'active', nextRunAt: 2_000_000_000,
    lastRunAt: null, runCount: 1, lastError: null, source: 'user', createdAt: 1, updatedAt: 1,
  }
}

function run(patch: Pick<ScheduledTaskRun, 'id' | 'status'> & Partial<ScheduledTaskRun>): ScheduledTaskRun {
  return {
    taskId: 'task', trigger: 'schedule', scheduledAt: 1, conversationId: 'conversation', error: null,
    createdAt: 1, startedAt: null, finishedAt: null, ...patch,
  }
}

function emit(event: ScheduledTasksChangedEvent) {
  for (const listener of [...listeners]) listener(event)
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(yes => { resolve = yes })
  return { promise, resolve }
}

let useScheduledTasks: typeof ScheduledTasksHook
let unmountView: (() => void) | undefined

function mount() {
  const view = renderHook(() => useScheduledTasks())
  unmountView = view.unmount
  return view
}

async function leave() {
  unmountView?.()
  unmountView = undefined
  await act(async () => { await Promise.resolve() })
}

beforeEach(async () => {
  listeners.clear()
  vi.resetModules()
  api.scheduledTasksList.mockReset()
  api.scheduledTaskRuns.mockReset()
  api.onScheduledTasksChanged.mockReset()
  api.onScheduledTasksChanged.mockImplementation(async (handler: (event: ScheduledTasksChangedEvent) => void) => {
    listeners.add(handler)
    return () => { listeners.delete(handler) }
  })
  api.scheduledTasksList.mockResolvedValue([task()])
  api.scheduledTaskRuns.mockResolvedValue([])
  // Module loading is intentional here: each case exercises a fresh subscription lifecycle.
  useScheduledTasks = (await import('./useScheduledTasks')).useScheduledTasks
})

afterEach(async () => { await leave() })

describe('useScheduledTasks live runs', () => {
  it('restores running and queued markers from startup history after the last subscriber leaves', async () => {
    api.scheduledTasksList.mockResolvedValue([task('alpha'), task('beta')])
    api.scheduledTaskRuns.mockImplementation(async (id: string) => {
      if (id === 'alpha') return [run({ id: 'alpha-live', taskId: 'alpha', status: 'running', createdAt: 10, startedAt: 12 })]
      return [run({ id: 'beta-done', taskId: 'beta', status: 'succeeded', createdAt: 8, finishedAt: 9 })]
    })
    const first = mount()
    await waitFor(() => expect(first.result.current.liveRuns.get('alpha')?.id).toBe('alpha-live'))
    expect(first.result.current.liveRuns.has('beta')).toBe(false)

    await leave()
    api.scheduledTaskRuns.mockImplementation(async (id: string) => {
      if (id === 'beta') return [run({ id: 'beta-live', taskId: 'beta', status: 'queued', createdAt: 30 })]
      return [run({ id: 'alpha-done', taskId: 'alpha', status: 'succeeded', createdAt: 10, finishedAt: 40 })]
    })
    const second = mount()
    await waitFor(() => expect(second.result.current.liveRuns.get('beta')?.status).toBe('queued'))
    expect(second.result.current.liveRuns.has('alpha')).toBe(false)
  })

  it('uses the run snapshot from after the listener is active', async () => {
    let listening = false
    let release!: () => void
    const gate = new Promise<void>(resolve => { release = resolve })
    api.onScheduledTasksChanged.mockImplementation(async (handler: (event: ScheduledTasksChangedEvent) => void) => {
      await gate
      listening = true
      listeners.add(handler)
      return () => { listeners.delete(handler) }
    })
    api.scheduledTaskRuns.mockImplementation(async () => listening
      ? [run({ id: 'current', status: 'queued', createdAt: 20 })]
      : [run({ id: 'stale', status: 'running', createdAt: 5, startedAt: 6 })])
    const view = mount()
    await act(async () => { release() })
    await waitFor(() => expect(view.result.current.liveRuns.get('task')?.id).toBe('current'))
    expect(view.result.current.liveRuns.get('task')?.status).toBe('queued')
  })

  it('does not let a late running history resurrect a run that already finished', async () => {
    const history = deferred<ScheduledTaskRun[]>()
    let requested = false
    const running = run({ id: 'run-a', status: 'running', createdAt: 10, startedAt: 11 })
    api.scheduledTaskRuns.mockImplementation(() => {
      requested = true
      return history.promise
    })
    const view = mount()
    await waitFor(() => expect(requested).toBe(true))
    act(() => emit({ taskId: 'task', run: { ...running, status: 'succeeded', finishedAt: 30 } }))
    await act(async () => { history.resolve([running]); await history.promise })
    await waitFor(() => expect(view.result.current.tasks).toHaveLength(1))
    expect(view.result.current.liveRuns.has('task')).toBe(false)
  })

  it('keeps a new live run when startup history still describes the previous one', async () => {
    const history = deferred<ScheduledTaskRun[]>()
    let requested = false
    const previous = run({ id: 'previous', status: 'running', createdAt: 10, startedAt: 12 })
    const next = run({ id: 'next', status: 'queued', createdAt: 40 })
    api.scheduledTaskRuns.mockImplementation(() => {
      requested = true
      return history.promise
    })
    const view = mount()
    await waitFor(() => expect(requested).toBe(true))
    act(() => emit({ taskId: 'task', run: next }))
    await act(async () => { history.resolve([previous]); await history.promise })
    await waitFor(() => expect(view.result.current.tasks).toHaveLength(1))
    expect(view.result.current.liveRuns.get('task')?.id).toBe('next')
  })

  it('keeps the newest live observation when history contains several active runs', async () => {
    api.scheduledTaskRuns.mockResolvedValue([
      run({ id: 'queued-newer', status: 'queued', createdAt: 50 }),
      run({ id: 'running-older', status: 'running', createdAt: 10, startedAt: 20 }),
    ])
    const newerQueued = mount()
    await waitFor(() => expect(newerQueued.result.current.liveRuns.get('task')?.id).toBe('queued-newer'))
    await leave()

    api.scheduledTaskRuns.mockResolvedValue([
      run({ id: 'queued-older', status: 'queued', createdAt: 15 }),
      run({ id: 'running-newer', status: 'running', createdAt: 10, startedAt: 80 }),
    ])
    const newerRunning = mount()
    await waitFor(() => expect(newerRunning.result.current.liveRuns.get('task')?.id).toBe('running-newer'))
  })

  it('does not restore a finished run from history when a later change reloads the task list', async () => {
    const running = run({ id: 'run-a', status: 'running', createdAt: 10, startedAt: 11 })
    const reloaded = deferred<ScheduledTask[]>()
    let lists = 0
    api.scheduledTasksList.mockImplementation(() => {
      lists += 1
      return lists === 1 ? Promise.resolve([task()]) : reloaded.promise
    })
    api.scheduledTaskRuns.mockResolvedValue([running])
    const view = mount()
    await waitFor(() => expect(view.result.current.liveRuns.get('task')?.status).toBe('running'))
    act(() => emit({ taskId: 'task', run: { ...running, status: 'failed', finishedAt: 19, error: 'stopped' } }))
    await waitFor(() => expect(lists).toBe(2))
    await act(async () => {
      reloaded.resolve([task()])
      await reloaded.promise
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(view.result.current.liveRuns.has('task')).toBe(false)
  })

  it('ignores a startup snapshot from the subscription that already closed', async () => {
    const firstRuns = deferred<ScheduledTaskRun[]>()
    const secondRuns = deferred<ScheduledTaskRun[]>()
    let reads = 0
    api.scheduledTaskRuns.mockImplementation(() => {
      reads += 1
      return reads === 1 ? firstRuns.promise : secondRuns.promise
    })
    mount()
    await waitFor(() => expect(reads).toBe(1))
    await leave()
    const second = mount()
    await waitFor(() => expect(reads).toBe(2))
    const fresh = run({ id: 'fresh', status: 'queued', createdAt: 8 })
    await act(async () => { secondRuns.resolve([fresh]); await secondRuns.promise })
    await waitFor(() => expect(second.result.current.liveRuns.get('task')?.id).toBe('fresh'))
    await act(async () => {
      firstRuns.resolve([run({ id: 'stale', status: 'running', createdAt: 3, startedAt: 4 })])
      await firstRuns.promise
      await Promise.resolve()
    })
    expect(second.result.current.liveRuns.get('task')?.id).toBe('fresh')
    expect(second.result.current.error).toBe('')
  })

  it('retries a failed task list and a failed startup history through the existing refresh', async () => {
    api.scheduledTasksList.mockRejectedValueOnce('list down')
    const view = mount()
    await waitFor(() => expect(view.result.current.error).toBe('list down'))
    expect(view.result.current.tasks).toBeNull()
    const queued = run({ id: 'after-list', status: 'queued', createdAt: 4 })
    api.scheduledTasksList.mockResolvedValue([task()])
    api.scheduledTaskRuns.mockResolvedValue([queued])
    act(() => { view.result.current.refresh() })
    await waitFor(() => expect(view.result.current.liveRuns.get('task')?.id).toBe('after-list'))
    expect(view.result.current.error).toBe('')

    await leave()
    api.scheduledTasksList.mockResolvedValue([task()])
    api.scheduledTaskRuns.mockRejectedValueOnce('runs down')
    const again = mount()
    await waitFor(() => expect(again.result.current.error).toBe('runs down'))
    expect(again.result.current.tasks?.map(item => item.id)).toEqual(['task'])
    expect(again.result.current.liveRuns.has('task')).toBe(false)
    await act(async () => { await Promise.resolve() })
    expect(again.result.current.error).toBe('runs down')
    api.scheduledTaskRuns.mockResolvedValue([run({ id: 'after-runs', status: 'running', createdAt: 6, startedAt: 7 })])
    act(() => { again.result.current.refresh() })
    await waitFor(() => expect(again.result.current.liveRuns.get('task')?.status).toBe('running'))
    expect(again.result.current.error).toBe('')
  })
})
