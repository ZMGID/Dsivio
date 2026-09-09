import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../api/tauri'
import type { TaskOrganizations, TaskOrganizationPatch } from './taskLibraryModel'

export function useTaskLibrary(domain: 'image' | 'video', native: boolean) {
  const [organization, setOrganization] = useState<TaskOrganizations>({})
  const [error, setError] = useState('')
  const [pending, setPending] = useState(false)
  const [ready, setReady] = useState(!native)
  const generation = useRef(0)
  const saving = useRef(false)
  const refresh = useCallback(async () => {
    if (!native || saving.current) return
    const ticket = ++generation.current
    try {
      const result = await api.studioTaskLibrary(domain)
      if (ticket === generation.current) { setOrganization(result); setError(''); setReady(true) }
    } catch (e) {
      if (ticket === generation.current) setError(`任务整理信息读取失败：${String(e)}`)
    }
  }, [domain, native])
  useEffect(() => {
    const requests = generation
    void refresh()
    const focus = () => { void refresh() }
    window.addEventListener('focus', focus)
    const timer = window.setInterval(() => { if (document.visibilityState !== 'hidden') void refresh() }, 5000)
    return () => { requests.current++; window.removeEventListener('focus', focus); window.clearInterval(timer) }
  }, [refresh])
  const update = async (ids: string[], patch: TaskOrganizationPatch) => {
    if (!native || saving.current || !ready) return false
    saving.current = true
    generation.current++
    setPending(true)
    setError('')
    try {
      setOrganization(await api.studioTaskLibrary(domain, ids, patch))
      return true
    } catch (e) {
      setError(String(e))
      return false
    } finally { saving.current = false; setPending(false) }
  }
  return { organization, error, pending, ready, refresh, update }
}
export type TaskLibraryState = ReturnType<typeof useTaskLibrary>
