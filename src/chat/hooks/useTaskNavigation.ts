import { draftKey, updateComposerDraft } from '../composerDraft'
import { useCallback, useRef } from 'react'

/** The tasks editor owns dirty state; every navigation awaits that same guard. */
export function useTaskNavigation() {
  const tasksLeaveGuardRef = useRef<(() => Promise<boolean>) | null>(null)
  const registerTasksLeaveGuard = useCallback((guard: (() => Promise<boolean>) | null) => {
    tasksLeaveGuardRef.current = guard
  }, [])
  const requestTasksLeave = useCallback(async () => await tasksLeaveGuardRef.current?.() ?? true, [])
  const createTaskByChat = useCallback(async (prompt: string, startNewConversation: () => void) => {
    if (!await requestTasksLeave()) return
    startNewConversation()
    updateComposerDraft(draftKey(null), draft => ({ ...draft, input: prompt }))
  }, [requestTasksLeave])
  return { tasksLeaveGuardRef, registerTasksLeaveGuard, requestTasksLeave, createTaskByChat }
}
