import { useCallback, useEffect, useRef, useState } from 'react'

/** The two editors share only request/selection lifecycle, never template format rules. */
export function useTemplateLibrary<T extends { id: string }>(list: () => Promise<T[]>) {
  const [items, setItems] = useState<T[]>([])
  const [editing, setEditing] = useState<T | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const alive = useRef(false)
  const running = useRef(false)
  const perform = useCallback(async (action: () => Promise<void>) => {
    if (running.current) return
    running.current = true
    setPending(true); setError(''); setNotice('')
    try { await action() } catch (e) { if (alive.current) setError(String(e)) }
    finally { running.current = false; if (alive.current) setPending(false) }
  }, [])
  useEffect(() => {
    alive.current = true
    let cancelled = false
    setPending(true)
    void list().then(values => { if (!cancelled) setItems(values) })
      .catch(e => { if (!cancelled) setError(String(e)) })
      .finally(() => { if (!cancelled) setPending(false) })
    return () => { cancelled = true; alive.current = false }
  }, [list])
  const accept = (template: T) => {
    if (!alive.current) return
    setItems(all => [...all.filter(t => t.id !== template.id), template])
    setEditing(structuredClone(template))
  }
  return { items, editing, setEditing, pending, error, notice, setNotice, perform, accept,
    refresh: () => perform(async () => { const values = await list(); if (alive.current) setItems(values) }) }
}
