/** @vitest-environment jsdom */
import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { blankWorkflow } from './workflowModel'
import { useWorkflowEditor } from './useWorkflowEditor'
import { workflowStore } from './workflowStore'

describe('useWorkflowEditor', () => {
  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.useRealTimers()
    localStorage.removeItem('kivio.workbench.workflows')
  })

  it('ignores an unchanged snapshot and debounces text edits until flush', () => {
    vi.useFakeTimers()
    const { result } = renderHook(() => useWorkflowEditor(blankWorkflow('原稿')))
    const id = result.current.flow.id
    act(() => result.current.change(result.current.flow))
    expect(result.current.canUndo).toBe(false)
    expect(result.current.dirty).toBe(false)
    expect(workflowStore.get(id)).toBeNull()

    act(() => result.current.change(flow => ({ ...flow, name: '第一稿' }), 'name'))
    act(() => result.current.change(flow => ({ ...flow, name: '第二稿' }), 'name'))
    expect(result.current.dirty).toBe(true)
    expect(result.current.canUndo).toBe(true)
    expect(workflowStore.get(id)).toBeNull()
    act(() => { vi.advanceTimersByTime(299) })
    expect(workflowStore.get(id)).toBeNull()
    act(() => { vi.advanceTimersByTime(1) })
    expect(workflowStore.get(id)?.name).toBe('第二稿')
    expect(result.current.dirty).toBe(false)

    act(() => result.current.change(flow => ({ ...flow, name: '未落盘' }), 'name'))
    act(() => result.current.endGroup())
    expect(workflowStore.get(id)?.name).toBe('未落盘')
    expect(result.current.dirty).toBe(false)
    act(() => result.current.undo())
    expect(result.current.flow.name).toBe('原稿')
    expect(result.current.canUndo).toBe(false)
    expect(result.current.canRedo).toBe(true)
    expect(workflowStore.get(id)?.name).toBe('原稿')
    act(() => result.current.redo())
    expect(result.current.flow.name).toBe('未落盘')
    expect(result.current.canRedo).toBe(false)
    expect(workflowStore.get(id)?.name).toBe('未落盘')
  })

  it('keeps a drag dirty without writing until it is persisted', () => {
    const { result } = renderHook(() => useWorkflowEditor(blankWorkflow('原稿')))
    const id = result.current.flow.id
    act(() => result.current.change(flow => ({ ...flow, name: '拖动中' }), 'drag:selection', false))
    expect(result.current.dirty).toBe(true)
    expect(result.current.canUndo).toBe(true)
    expect(workflowStore.get(id)).toBeNull()
    act(() => result.current.change(flow => ({ ...flow, name: '已放下' }), 'drag:selection'))
    expect(result.current.dirty).toBe(false)
    expect(workflowStore.get(id)?.name).toBe('已放下')
  })

  it('flushes a deferred text edit when the editor unmounts', () => {
    const { result, unmount } = renderHook(() => useWorkflowEditor(blankWorkflow('原稿')))
    const id = result.current.flow.id
    act(() => result.current.change(flow => ({ ...flow, name: '离开前' }), 'title'))
    expect(workflowStore.get(id)).toBeNull()
    unmount()
    expect(workflowStore.get(id)?.name).toBe('离开前')
  })
})

  it('keeps a failed autosave dirty and retries it when leaving', () => {
    vi.useFakeTimers()
    const original = blankWorkflow('原稿')
    workflowStore.save(original)
    const hook = renderHook(() => useWorkflowEditor(original))
    vi.spyOn(workflowStore, 'save').mockImplementationOnce(() => { throw new Error('temporary storage failure') })
    act(() => hook.result.current.change(flow => ({ ...flow, name: '改稿' }), 'title'))
    act(() => { vi.advanceTimersByTime(300) })
    expect(hook.result.current.saveError).toContain('temporary storage failure')
    expect(hook.result.current.dirty).toBe(true)
    hook.unmount()
    expect(workflowStore.get(original.id)?.name).toBe('改稿')
  })

  it('keeps unsaved changes across leaving and reopening while storage is unavailable', () => {
    vi.useFakeTimers()
    const original = blankWorkflow('原稿')
    workflowStore.save(original)
    const write = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('quota exceeded') })
    const hook = renderHook(() => useWorkflowEditor(original))
    act(() => hook.result.current.change(flow => ({ ...flow, name: '保留的草稿' }), 'title'))
    act(() => { vi.advanceTimersByTime(300) })
    hook.unmount()
    const recovered = workflowStore.get(original.id)!
    expect(recovered.name).toBe('保留的草稿')
    const reopened = renderHook(() => useWorkflowEditor(recovered))
    expect(reopened.result.current.dirty).toBe(true)
    expect(reopened.result.current.saveError).toContain('quota exceeded')
    write.mockRestore()
    act(() => { expect(reopened.result.current.save()).toBe(true) })
    expect(reopened.result.current.dirty).toBe(false)
    expect(reopened.result.current.saveError).toBe('')
    reopened.unmount()
    expect(workflowStore.get(original.id)?.name).toBe('保留的草稿')
    workflowStore.remove(original.id)
  })
