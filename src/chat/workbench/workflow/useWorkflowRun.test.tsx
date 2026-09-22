/** @vitest-environment jsdom */
import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { useWorkflowRun } from './useWorkflowRun'
import { blankWorkflow } from './workflowModel'
import type { WorkflowRun } from '../../../generated/generationWorkflow'
import { api } from '../../../api/tauri'
import { checkWorkflow } from './workflowValidation'
vi.mock('../../../api/tauri', () => ({ isTauriRuntime: () => true, api: { listWorkflowRuns: vi.fn(), startWorkflowRun: vi.fn(), resumeWorkflowRun: vi.fn(), cancelWorkflowRun: vi.fn() } }))
vi.mock('./workflowValidation', () => ({ checkWorkflow: vi.fn() }))
const flow = blankWorkflow('workflow')
const run: WorkflowRun = { id: 'run', workflow: flow, status: 'running', createdAt: 'now', updatedAt: 'now', nodes: [], error: null }
beforeEach(() => { vi.mocked(api.listWorkflowRuns).mockResolvedValue([]); vi.mocked(checkWorkflow).mockResolvedValue([]) })
afterEach(() => { cleanup(); vi.resetAllMocks(); vi.useRealTimers() })
it('holds a synchronous submit lock across validation and IPC, then reopens the durable run', async () => {
  let complete!: (r: WorkflowRun) => void
  vi.mocked(api.startWorkflowRun).mockImplementation(() => new Promise(resolve => { complete = resolve }))
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let first!: Promise<void>
  act(() => { first = hook.result.current.start(flow); void hook.result.current.start(flow) })
  await waitFor(() => expect(api.startWorkflowRun).toHaveBeenCalledTimes(1))
  expect(hook.result.current.pending).toBe(true)
  await act(async () => { complete(run); await first })
  expect(hook.result.current.selected?.id).toBe('run')
  expect(hook.result.current.busy).toBe(true)
  hook.unmount()
  expect(api.cancelWorkflowRun).not.toHaveBeenCalled()
  vi.mocked(api.listWorkflowRuns).mockResolvedValue([run])
  const reopened = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(reopened.result.current.active?.id).toBe('run'))
  await act(async () => { await reopened.result.current.start(flow) })
  expect(api.startWorkflowRun).toHaveBeenCalledTimes(1)
})
it('keeps submission errors visible after history refresh and allows retry', async () => {
  vi.mocked(api.startWorkflowRun).mockRejectedValueOnce(new Error('disk full')).mockResolvedValueOnce(run)
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  await act(async () => { await hook.result.current.start(flow) })
  expect(hook.result.current.error).toContain('disk full')
  expect(hook.result.current.pending).toBe(false)
  await act(async () => { await hook.result.current.refresh() })
  expect(hook.result.current.error).toContain('disk full')
  await act(async () => { await hook.result.current.start(flow) })
  expect(hook.result.current.selected?.id).toBe('run')
  expect(hook.result.current.error).toBe('')
})
it('discards a pre-submit list response that arrives after the submitted run', async () => {
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let late!: (runs: WorkflowRun[]) => void
  vi.mocked(api.listWorkflowRuns).mockImplementationOnce(() => new Promise(resolve => { late = resolve }))
  let reading!: Promise<void>
  act(() => { reading = hook.result.current.refresh() })
  vi.mocked(api.startWorkflowRun).mockResolvedValue(run)
  await act(async () => { await hook.result.current.start(flow) })
  await act(async () => { late([]); await reading })
  expect(hook.result.current.active?.id).toBe('run')
})
it('never submits an invalid graph and resumes a stored snapshot by its run id', async () => {
  vi.mocked(checkWorkflow).mockResolvedValue(['input missing'])
  const hook = renderHook(() => useWorkflowRun(flow.id))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  await act(async () => { await hook.result.current.start(flow) })
  expect(api.startWorkflowRun).not.toHaveBeenCalled()
  expect(hook.result.current.error).toContain('input missing')
  vi.mocked(api.resumeWorkflowRun).mockResolvedValue(run)
  await act(async () => { await hook.result.current.resume('run') })
  expect(api.resumeWorkflowRun).toHaveBeenCalledWith('run')
})
