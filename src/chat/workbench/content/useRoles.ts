import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../../api/tauri'
import type { Role, RoleSaveRequest } from '../../../generated/roles'

/**
 * Saved roles. A newer load or unmount drops a late list/save so reopening
 * shows the store, not a response from the previous visit.
 */
export function useRoles() {
  const [roles, setRoles] = useState<Role[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const generation = useRef(0)
  const mounted = useRef(true)

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      generation.current += 1
    }
  }, [])

  const refresh = useCallback(async () => {
    const gen = ++generation.current
    setLoading(true)
    setError(null)
    try {
      const next = await api.rolesList()
      if (!mounted.current || gen !== generation.current) return next
      setRoles(next)
      setError(null)
      return next
    } catch (err) {
      if (!mounted.current || gen !== generation.current) return []
      setError(err instanceof Error ? err.message : String(err))
      return []
    } finally {
      if (mounted.current && gen === generation.current) setLoading(false)
    }
  }, [])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const save = useCallback(async (request: RoleSaveRequest) => {
    const gen = ++generation.current
    const saved = await api.rolesSave(request)
    if (!mounted.current || gen !== generation.current) return saved
    setRoles((current) => {
      const rest = current.filter((role) => role.id !== saved.id)
      return [saved, ...rest].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    })
    setError(null)
    return saved
  }, [])

  const remove = useCallback(async (id: string) => {
    const gen = ++generation.current
    await api.rolesDelete(id)
    if (!mounted.current || gen !== generation.current) return
    setRoles((current) => current.filter((role) => role.id !== id))
  }, [])

  return { roles, loading, error, refresh, save, remove }
}
