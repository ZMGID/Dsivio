// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { useTaskNavigation } from './useTaskNavigation'
import { draftKey, getComposerDraft, setComposerDraft } from '../composerDraft'

it('honors unsaved-task cancellation and only prefills a new chat after leaving is accepted', async () => {
  const key = draftKey(null)
  setComposerDraft(key, { input: '原草稿', quotes: [], attachments: [] })
  const start = vi.fn()
  const { result } = renderHook(() => useTaskNavigation())
  const guard = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true)
  result.current.registerTasksLeaveGuard(guard)
  await act(async () => { await result.current.createTaskByChat('创建巡店任务', start) })
  expect(start).not.toHaveBeenCalled()
  expect(getComposerDraft(key)?.input).toBe('原草稿')
  await act(async () => { await result.current.createTaskByChat('创建巡店任务', start) })
  expect(start).toHaveBeenCalledTimes(1)
  expect(getComposerDraft(key)?.input).toBe('创建巡店任务')
  setComposerDraft(key, { input: '', quotes: [], attachments: [] })
})
