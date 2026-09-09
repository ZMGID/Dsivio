import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

type Envelope<T> = { revision: number; value: T | null }
// Rust JSON maps sort keys; compare content, not insertion order across runtimes.
function fingerprint(value: unknown): string {
  return JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item)
    ? Object.fromEntries(Object.entries(item).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) : item)
}

/** One shared draft per workflow. New shared revisions are adopted automatically. */
export function useSharedDraft<T extends { brief: unknown }>(domain: 'image' | 'video', entry: string, enabled: boolean, value: T, apply: (value: T) => void | Promise<void>, paused = false) {
  const entries = useRef(new Map<string, { value: T; apply: (value: T) => void | Promise<void>; paused: boolean }>())
  entries.current.set(`${domain}/${entry}`, { value, apply, paused })
  const baselines = useRef(new Map<string, { revision?: number; acknowledged: string; pending: Promise<void> }>())
  const [message, setMessage] = useState('')
  const requestSync = useRef<() => void>(() => {})
  useEffect(() => {
    if (!enabled) return
    const snapshot = () => entries.current.get(`${domain}/${entry}`)!
    let stopped = false
    let inFlight = false
    let queued = false
    let adopting = false
    const key = `${domain}/${entry}`
    let baseline = baselines.current.get(key)
    if (!baseline) {
      baseline = { acknowledged: fingerprint(snapshot().value), pending: Promise.resolve() }
      baselines.current.set(key, baseline)
    }
    const state = baseline
    setMessage('')
    let pending: Promise<void> = Promise.resolve()
    const read = () => invoke<Envelope<T>>('studio_draft', { domain, entry, revision: null, value: null })
    const adopt = async (result: Envelope<T>) => {
      adopting = true
      try {
        if (result.value) {
          const previous = snapshot()
          await previous.apply(result.value)
          // React may not have committed the setter yet. Do not let a timer
          // publish the pre-adoption value back over the shared draft.
          if (snapshot() === previous) entries.current.set(key, { ...previous, value: result.value })
        }
        state.revision = result.revision
        if (result.value) state.acknowledged = fingerprint(result.value)
      } finally { adopting = false }
    }
    const sync = async () => {
      if (inFlight || stopped || snapshot().paused) return
      inFlight = true
      try {
        const result = await read()
        if (stopped || snapshot().paused) return
        const current = fingerprint(snapshot().value)
        const changedRemotely = state.revision === undefined || result.revision !== state.revision
        if (changedRemotely) {
          await adopt(result)
          setMessage('')
        }
        if (stopped) return
        // Wait for React to apply remote state before considering another write.
        if (changedRemotely && result.value) return
        if (!result.value || current !== fingerprint(result.value)) {
          const saved = await invoke<Envelope<T>>('studio_draft', { domain, entry, revision: result.revision, value: snapshot().value })
          state.revision = saved.revision
          state.acknowledged = fingerprint(saved.value)
          if (!stopped) setMessage('')
        }
      } catch {
        // A rejected stale write is normal synchronization, not a user decision.
        // Re-read and use the shared version without flashing an error first.
        try {
          const latest = await read()
          if (stopped || snapshot().paused) return
          if (latest.revision !== state.revision) {
            await adopt(latest)
            setMessage('')
            return
          }
        } catch { /* Actual I/O failures retry on the next tick. */ }
        if (!stopped) setMessage('共享草稿尚未同步，本地编辑已保留；正在重试。')
      } finally { inFlight = false }
    }
    const request = () => {
      if (inFlight || queued || stopped) return
      queued = true
      pending = state.pending.then(async () => { queued = false; await sync() })
    }
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
      state.pending = pending.then(async () => {
        if (skipFlush || state.revision === undefined || fingerprint(finalValue) === state.acknowledged) return
        try {
          const saved = await invoke<Envelope<T>>('studio_draft', { domain, entry, revision: state.revision, value: finalValue })
          state.revision = saved.revision
          state.acknowledged = fingerprint(saved.value)
        } catch { /* Never overwrite a newer shared draft. */ }
      })
    }
  }, [domain, entry, enabled])
  useEffect(() => { requestSync.current() }, [value])
  return { message }
}
