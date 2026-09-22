import { useRef, useState } from 'react'
import type { GenerationWorkflow } from './workflowModel'
import { workflowStore } from './workflowStore'

/** Owns the current snapshot, coalesced history and durable save status together. */
export function useWorkflowEditor(initial: GenerationWorkflow) {
  const current = useRef(initial)
  const past = useRef<GenerationWorkflow[]>([]), future = useRef<GenerationWorkflow[]>([])
  const group = useRef({ key: '', time: 0 })
  const [flow, setFlow] = useState(initial)
  const [saveError, setSaveError] = useState('')
  function save(next = current.current) {
    try { workflowStore.save(next); setSaveError(''); return true }
    catch (error) { setSaveError(`保存失败：${String(error)}`); return false }
  }
  function show(next: GenerationWorkflow, persist = true) {
    current.current = next; setFlow(next)
    if (persist) save(next)
  }
  function change(update: GenerationWorkflow | ((flow: GenerationWorkflow) => GenerationWorkflow), key = '', persist = true) {
    const next = typeof update === 'function' ? update(current.current) : update
    if (JSON.stringify(next) === JSON.stringify(current.current)) return
    const now = Date.now()
    if (!key || group.current.key !== key || (!key.startsWith('drag:') && now - group.current.time > 800)) {
      past.current = [...past.current.slice(-99), current.current]
    }
    group.current = { key, time: now }; future.current = []
    show(next, persist)
  }
  function endGroup() { group.current = { key: '', time: 0 } }
  function undo() {
    const next = past.current.pop()
    if (!next) return
    future.current.push(current.current); endGroup(); show(next)
  }
  function redo() {
    const next = future.current.pop()
    if (!next) return
    past.current.push(current.current); endGroup(); show(next)
  }
  return { flow, change, undo, redo, endGroup, save, saveError, canUndo: past.current.length > 0, canRedo: future.current.length > 0 }
}

export function isTextEditing(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest('input, textarea, select, [contenteditable="true"], [role="textbox"], [role="combobox"], [role="listbox"]')
}
