import { useEffect, useRef, useState } from 'react'
import type { GenerationWorkflow } from './workflowModel'
import { workflowStore } from './workflowStore'

const TEXT_SAVE_MS = 300

function isTextGroup(key: string): boolean {
  return key.length > 0 && !key.startsWith('drag:') && !key.startsWith('move:')
}

/** Owns the current snapshot, coalesced history and durable save status together. */
export function useWorkflowEditor(initial: GenerationWorkflow) {
  const current = useRef(initial)
  const past = useRef<GenerationWorkflow[]>([])
  const future = useRef<GenerationWorkflow[]>([])
  const group = useRef({ key: '', time: 0 })
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dragging = useRef(false)
  const dirtyRef = useRef(Boolean(workflowStore.saveError(initial.id)))
  const [flow, setFlow] = useState(initial)
  const [saveError, setSaveError] = useState(() => workflowStore.saveError(initial.id))
  const [dirty, setDirty] = useState(() => Boolean(workflowStore.saveError(initial.id)))
  const [history, setHistory] = useState({ undo: 0, redo: 0 })

  function markDirty(value: boolean) {
    dirtyRef.current = value
    setDirty(prev => prev === value ? prev : value)
  }
  function publishHistory() {
    const undo = past.current.length
    const redo = future.current.length
    setHistory(prev => prev.undo === undo && prev.redo === redo ? prev : { undo, redo })
  }
  function cancelTimer() {
    if (timer.current !== null) {
      clearTimeout(timer.current)
      timer.current = null
    }
  }
  function write(next: GenerationWorkflow) {
    try {
      workflowStore.save(next)
      setSaveError('')
      return true
    } catch (error) {
      setSaveError(`保存失败：${String(error)}`)
      return false
    }
  }
  function save(next = current.current) {
    cancelTimer()
    dragging.current = false
    const ok = write(next)
    markDirty(!ok)
    return ok
  }
  function scheduleTextSave() {
    cancelTimer()
    markDirty(true)
    timer.current = setTimeout(() => {
      timer.current = null
      const ok = write(current.current)
      if (!dragging.current) markDirty(!ok)
    }, TEXT_SAVE_MS)
  }
  function change(update: GenerationWorkflow | ((flow: GenerationWorkflow) => GenerationWorkflow), key = '', persist = true) {
    const next = typeof update === 'function' ? update(current.current) : update
    if (next === current.current) return
    const now = Date.now()
    if (!key || group.current.key !== key || (!key.startsWith('drag:') && now - group.current.time > 800)) {
      past.current = [...past.current.slice(-99), current.current]
    }
    group.current = { key, time: now }
    future.current = []
    publishHistory()
    current.current = next
    setFlow(next)
    if (!persist) {
      dragging.current = true
      markDirty(true)
      return
    }
    dragging.current = false
    if (isTextGroup(key)) {
      scheduleTextSave()
      return
    }
    save(next)
  }
  function endGroup() {
    group.current = { key: '', time: 0 }
    if (timer.current !== null) save()
  }
  function undo() {
    if (timer.current !== null) save()
    const next = past.current.pop()
    if (!next) return
    future.current.push(current.current)
    group.current = { key: '', time: 0 }
    publishHistory()
    current.current = next
    setFlow(next)
    save(next)
  }
  function redo() {
    if (timer.current !== null) save()
    const next = future.current.pop()
    if (!next) return
    past.current.push(current.current)
    group.current = { key: '', time: 0 }
    publishHistory()
    current.current = next
    setFlow(next)
    save(next)
  }
  useEffect(() => () => {
    if (timer.current === null && !dirtyRef.current) return
    if (timer.current !== null) clearTimeout(timer.current)
    timer.current = null
    try { workflowStore.save(current.current) } catch { /* leaving the editor */ }
  }, [])
  return { flow, change, undo, redo, endGroup, save, saveError, dirty, canUndo: history.undo > 0, canRedo: history.redo > 0 }
}

export function isTextEditing(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest('input, textarea, select, [contenteditable="true"], [role="textbox"], [role="combobox"], [role="listbox"]')
}
