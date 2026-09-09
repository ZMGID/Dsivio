import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

type Envelope<T> = { revision: number; value: T | null }
// Rust JSON maps sort keys; compare content, not insertion order across runtimes.
function fingerprint(value: unknown): string {
  return JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item)
    ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : item)
}

/** CAS protects edits in either entry. Local storage remains an offline recovery copy. */
export function useSharedDraft<T extends { brief: unknown }>(domain: 'image' | 'video', entry: string, enabled: boolean, value: T, apply: (value: T) => void | Promise<void>, paused = false) {
  const entries = useRef(new Map<string, { value: T; apply: (value: T) => void | Promise<void>; paused: boolean }>())
  entries.current.set(`${domain}/${entry}`, { value, apply, paused })
  const [message, setMessage] = useState('')
  const [hasConflict, setHasConflict] = useState(false)
  const keep = useRef<() => void>(() => {})
  const reload = useRef<() => void>(() => {})
  const requestSync = useRef<() => void>(() => {})
  useEffect(() => {
    if (!enabled) return
    const snapshot = () => entries.current.get(`${domain}/${entry}`)!
    let stopped = false
    let inFlight = false
    let adopting = false
    let revision: number | undefined
    let acknowledged = fingerprint(snapshot().value)
    let conflict = false
    let remote: Envelope<T> | undefined
    let pending: Promise<void> = Promise.resolve()
    const read = () => invoke<Envelope<T>>('studio_draft', { domain, entry, revision: null, value: null })
    const adopt = async (result: Envelope<T>) => {
      adopting = true
      try {
        if (result.value) await snapshot().apply(result.value)
        revision = result.revision
        if (result.value) acknowledged = fingerprint(result.value)
      } finally { adopting = false }
    }
    keep.current = () => {
      if (!remote) return
      revision = remote.revision
      acknowledged = fingerprint(snapshot().value)
      conflict = false
      setHasConflict(false)
      setMessage('')
    }
    reload.current = () => {
      if (!remote || inFlight) return
      inFlight = true
      void adopt(remote).then(() => {
        conflict = false
        setHasConflict(false)
        setMessage('')
      }).catch(() => setMessage('无法载入共享任务，本地编辑已保留，请重试。'))
        .finally(() => { inFlight = false })
    }
    const sync = async () => {
      if (inFlight || stopped || snapshot().paused) return
      inFlight = true
      try {
        const result = await read()
        if (stopped || snapshot().paused) return
        remote = result
        const current = fingerprint(snapshot().value)
        const changedRemotely = revision === undefined || result.revision !== revision
        if (changedRemotely) {
          const brief = snapshot().value.brief as Record<string, unknown>
          const hasLocal = Boolean(brief.requirement || brief.request || (brief.products as unknown[])?.length || (brief.images as unknown[])?.length)
          if (result.value && current !== fingerprint(result.value) &&
            (current !== acknowledged || (revision === undefined && hasLocal))) {
            conflict = true
            setHasConflict(true)
            setMessage('聊天或其他页面已修改此草稿。本地编辑已保留，可选择载入共享版本。')
          } else {
            await adopt(result)
            conflict = false
            setHasConflict(false)
            setMessage('')
          }
        }
        if (conflict || stopped) return
        // Wait for React to apply remote state before considering another write.
        if (changedRemotely && result.value) return
        if (!result.value || current !== fingerprint(result.value)) {
          const saved = await invoke<Envelope<T>>('studio_draft', { domain, entry, revision: result.revision, value: snapshot().value })
          revision = saved.revision
          acknowledged = fingerprint(saved.value)
          if (!stopped) setMessage('')
        }
      } catch {
        if (!stopped) setMessage('共享草稿尚未同步，本地编辑已保留；正在重试。')
      } finally { inFlight = false }
    }
    const request = () => { if (!inFlight && !stopped) pending = sync() }
    requestSync.current = request
    request()
    const timer = window.setInterval(request, 1000)
    window.addEventListener('focus', request)
    return () => {
      const finalValue = snapshot().value
      const skipFlush = adopting
      stopped = true
      window.clearInterval(timer)
      window.removeEventListener('focus', request)
      // Finish an in-flight write before flushing the last keystroke on navigation.
      // A changed remote revision rejects this write; localStorage keeps recovery.
      void pending.then(async () => {
        if (skipFlush || conflict || revision === undefined || fingerprint(finalValue) === acknowledged) return
        try {
          await invoke('studio_draft', { domain, entry, revision, value: finalValue })
        } catch { /* Never overwrite a newer shared draft. */ }
      })
    }
  }, [domain, entry, enabled])
  useEffect(() => { requestSync.current() }, [value])
  return { message, hasConflict, reload: () => reload.current(), keep: () => keep.current() }
}
