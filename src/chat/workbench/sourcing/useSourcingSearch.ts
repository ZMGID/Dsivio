import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../../api/tauri'
import type { LookalikeRequest, SourcingSearch, SourcingSearchSummary } from '../../../generated/sourcing'

/** Owns search/history requests and rejects results from an abandoned page or selection. */
export function useSourcingSearch() {
  const [result, setResult] = useState<SourcingSearch | null>(null)
  const [history, setHistory] = useState<SourcingSearchSummary[]>([])
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const [historyPending, setHistoryPending] = useState(false)
  const [historyError, setHistoryError] = useState('')
  const generation = useRef(0), historyGeneration = useRef(0)
  const busy = useRef(false)
  const alive = useRef(false)
  const refreshHistory = useCallback(async () => {
    const version = ++historyGeneration.current
    setHistoryPending(true); setHistoryError('')
    try {
      const rows = await api.sourcingHistory()
      if (alive.current && version === historyGeneration.current) setHistory(rows)
    } catch (e) {
      if (alive.current && version === historyGeneration.current) setHistoryError(String(e))
    } finally {
      if (alive.current && version === historyGeneration.current) setHistoryPending(false)
    }
  }, [])
  useEffect(() => {
    alive.current = true
    generation.current++
    void refreshHistory()
    return () => { alive.current = false }
  }, [refreshHistory])
  const run = async (request: () => Promise<SourcingSearch>) => {
    if (busy.current) return false
    busy.current = true
    const version = ++generation.current
    setPending(true); setError(''); setResult(null)
    try {
      const value = await request()
      if (alive.current && generation.current === version) {
        setResult(value)
        // History is a separate read: its latency/failure must not fail a completed search.
        void refreshHistory()
        return true
      }
    } catch (e) { if (alive.current && generation.current === version) setError(String(e)) }
    finally {
      busy.current = false
      if (alive.current && generation.current === version) setPending(false)
    }
    return false
  }
  const clear = useCallback(() => { if (!busy.current) { generation.current++; setResult(null); setError('') } }, [])
  return {
    result, history, pending, error, historyPending, historyError,
    search: (request: LookalikeRequest) => run(() => api.sourcingSearch(request)),
    restore: (id: string) => run(() => api.sourcingGetSearch(id)),
    clear,
    refreshHistory,
  }
}
