/** @vitest-environment jsdom */
import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { listen } from '@tauri-apps/api/event'
import { useWorkflowRun } from './useWorkflowRun'
import { blankWorkflow } from './workflowModel'
import type { WorkflowRun, WorkflowStatus } from '../../../generated/generationWorkflow'
import { api } from '../../../api/tauri'
import { checkWorkflow } from './workflowValidation'

const runEvents = vi.hoisted(() => ({
  unlisten: vi.fn(),
  emit: undefined as unknown as (run: WorkflowRun) => void,
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }))
vi.mock('../../../api/tauri', () => ({ isTauriRuntime: () => true, api: { listWorkflowRuns: vi.fn(), startWorkflowRun: vi.fn(), resumeWorkflowRun: vi.fn(), cancelWorkflowRun: vi.fn() } }))
vi.mock('./workflowValidation', () => ({ checkWorkflow: vi.fn() }))

const flow = blankWorkflow('workflow')
const run: WorkflowRun = { id: 'run', workflow: flow, status: 'running', createdAt: 'now', updatedAt: 'now', nodes: [], error: null }
function node(status: WorkflowStatus) {
  return { nodeId: 'prompt', status, error: null, startedAt: null, finishedAt: null, mediaTaskId: null, outputs: {} }
}
beforeEach(() => {
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([])
  vi.mocked(checkWorkflow).mockResolvedValue([])
  runEvents.unlisten.mockReset()
  vi.mocked(listen).mockImplementation(async (_name, handler) => {
    runEvents.emit = (payload) => handler({ payload } as never)
    return runEvents.unlisten
  })
})
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.useRealTimers() })

it('holds a synchronous submit lock across validation and IPC, then reopens the durable run', async () => {
  let complete!: (r: WorkflowRun) => void
  vi.mocked(api.startWorkflowRun).mockImplementation(() => new Promise(resolve => { complete = resolve }))
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let first!: Promise<void>
  act(() => { first = hook.result.current.start(() => flow); void hook.result.current.start(() => flow) })
  await waitFor(() => expect(api.startWorkflowRun).toHaveBeenCalledTimes(1))
  expect(api.startWorkflowRun).toHaveBeenCalledWith(flow)
  expect(hook.result.current.pending).toBe(true)
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  await act(async () => { complete(run); await first })
  expect(hook.result.current.selected?.id).toBe('run')
  expect(hook.result.current.busy).toBe(true)
  hook.unmount()
  expect(api.cancelWorkflowRun).not.toHaveBeenCalled()
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  const reopened = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(reopened.result.current.active?.id).toBe('run'))
  await act(async () => { await reopened.result.current.start(() => flow) })
  expect(api.startWorkflowRun).toHaveBeenCalledTimes(1)
})

it('keeps submission errors visible after history refresh and allows retry', async () => {
  vi.mocked(api.startWorkflowRun).mockRejectedValueOnce(new Error('disk full')).mockResolvedValueOnce(run)
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  await act(async () => { await hook.result.current.start(() => flow) })
  expect(hook.result.current.error).toContain('disk full')
  expect(hook.result.current.pending).toBe(false)
  await act(async () => { await hook.result.current.refresh() })
  expect(hook.result.current.error).toContain('disk full')
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  await act(async () => { await hook.result.current.start(() => flow) })
  expect(hook.result.current.selected?.id).toBe('run')
  expect(hook.result.current.error).toBe('')
})

it('discards a pre-submit list response that arrives after the submitted run', async () => {
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let late!: (runs: WorkflowRun[]) => void
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  vi.mocked(api.listWorkflowRuns).mockImplementationOnce(() => new Promise(resolve => { late = resolve }))
  let reading!: Promise<void>
  act(() => { reading = hook.result.current.refresh() })
  vi.mocked(api.startWorkflowRun).mockResolvedValue(run)
  await act(async () => { await hook.result.current.start(() => flow) })
  await act(async () => { late([]); await reading })
  expect(hook.result.current.active?.id).toBe('run')
})

it('never submits an invalid graph and resumes a stored snapshot by its run id', async () => {
  vi.mocked(checkWorkflow).mockResolvedValue(['input missing'])
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  await act(async () => { await hook.result.current.start(() => flow) })
  expect(api.startWorkflowRun).not.toHaveBeenCalled()
  expect(hook.result.current.error).toContain('input missing')
  vi.mocked(api.resumeWorkflowRun).mockResolvedValue(run)
  await act(async () => { await hook.result.current.resume('run') })
  expect(api.resumeWorkflowRun).toHaveBeenCalledWith('run')
})

it('aborts start when the flow identity changes during the check', async () => {
  let release!: (issues: string[]) => void
  vi.mocked(checkWorkflow).mockImplementation(() => new Promise(resolve => { release = resolve }))
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let current = flow
  let starting!: Promise<void>
  act(() => { starting = hook.result.current.start(() => current) })
  current = { ...flow }
  await act(async () => { release([]); await starting })
  expect(api.startWorkflowRun).not.toHaveBeenCalled()
  expect(hook.result.current.error).toContain('检查期间配置发生变化，请重新运行')
})

it('applies a workflow-run-updated event to a running node without polling', async () => {
  const running = { ...run, nodes: [node('running')] }
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([running])
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.active?.nodes[0]?.status).toBe('running'))
  const calls = vi.mocked(api.listWorkflowRuns).mock.calls.length
  await act(async () => { runEvents.emit({ ...running, id: 'other', workflow: { ...flow, id: 'other' } }) })
  expect(hook.result.current.runs.some(item => item.id === 'other')).toBe(false)
  await act(async () => { runEvents.emit({ ...running, nodes: [node('succeeded')] }) })
  expect(hook.result.current.runs.filter(item => item.id === 'run')).toHaveLength(1)
  expect(hook.result.current.runs[0].nodes[0].status).toBe('succeeded')
  expect(api.listWorkflowRuns).toHaveBeenCalledTimes(calls)
})

it('unsubscribes from workflow run updates on unmount', async () => {
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(listen).toHaveBeenCalledWith('workflow-run-updated', expect.any(Function)))
  await act(async () => {})
  hook.unmount()
  expect(runEvents.unlisten).toHaveBeenCalled()
  await act(async () => { runEvents.emit(run) })
})

it('unsubscribes when unmounted before the listener resolves', async () => {
  let resolveListen: (stop: () => void) => void = () => {}
  vi.mocked(listen).mockImplementationOnce(() => new Promise(resolve => { resolveListen = resolve }))
  const hook = renderHook(() => useWorkflowRun(flow.id))
  hook.unmount()
  const stop = vi.fn()
  await act(async () => { resolveListen(stop) })
  expect(stop).toHaveBeenCalled()
})

it('refreshes after cancel so a void result replaces the active run', async () => {
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.active?.id).toBe('run'))
  vi.mocked(api.cancelWorkflowRun).mockResolvedValue(undefined)
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([{ ...run, status: 'cancelled' }])
  await act(async () => { await hook.result.current.cancel() })
  expect(api.cancelWorkflowRun).toHaveBeenCalledWith('run')
  expect(hook.result.current.active).toBeUndefined()
  expect(hook.result.current.runs[0].status).toBe('cancelled')
})

it('polls an active run only as a five-second fallback', async () => {
  vi.useFakeTimers()
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await act(async () => { await Promise.resolve() })
  expect(hook.result.current.active?.id).toBe('run')
  const calls = vi.mocked(api.listWorkflowRuns).mock.calls.length
  await act(async () => { await vi.advanceTimersByTimeAsync(4999) })
  expect(api.listWorkflowRuns).toHaveBeenCalledTimes(calls)
  await act(async () => { await vi.advanceTimersByTimeAsync(1) })
  expect(api.listWorkflowRuns).toHaveBeenCalledTimes(calls + 1)
})


it('preserves a completed event when the earlier start response arrives and refresh fails', async () => {
  let complete!: (r: WorkflowRun) => void
  vi.mocked(api.startWorkflowRun).mockImplementation(() => new Promise(resolve => { complete = resolve }))
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let starting!: Promise<void>
  act(() => { starting = hook.result.current.start(() => flow) })
  await waitFor(() => expect(api.startWorkflowRun).toHaveBeenCalledTimes(1))
  await act(async () => { runEvents.emit({ ...run, status: 'succeeded', updatedAt: 'later' }) })
  expect(hook.result.current.selected?.status).toBe('succeeded')
  vi.mocked(api.listWorkflowRuns).mockRejectedValue(new Error('temporary read error'))
  await act(async () => { complete(run); await starting })
  expect(hook.result.current.selected?.status).toBe('succeeded')
  expect(hook.result.current.busy).toBe(false)
})

it('discards an older list response after a newer refresh has completed', async () => {
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let complete!: (r: WorkflowRun[]) => void
  vi.mocked(api.listWorkflowRuns).mockImplementationOnce(() => new Promise(resolve => { complete = resolve }))
  let older!: Promise<void>
  act(() => { older = hook.result.current.refresh() })
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([{ ...run, status: 'succeeded' }])
  await act(async () => { await hook.result.current.refresh() })
  expect(hook.result.current.selected?.status).toBe('succeeded')
  await act(async () => { complete([run]); await older })
  expect(hook.result.current.selected?.status).toBe('succeeded')
  expect(hook.result.current.busy).toBe(false)
})
