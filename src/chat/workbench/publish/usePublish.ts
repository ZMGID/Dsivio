import { useCallback, useEffect, useRef, useState } from 'react'
import { api, isTauriRuntime } from '../../../api/tauri'
import type { PublishAccount, PublishRecord, PublishRequest, PublishSubmitResult, VideoStats } from '../../../generated/publish'

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function mergeRecords(previous: PublishRecord[], incoming: PublishRecord[]) {
  const byId = new Map(previous.map((record) => [record.id, record]))
  for (const record of incoming) byId.set(record.id, record)
  return [...byId.values()]
}

export function usePublish() {
  const alive = useRef(true)
  const submitGeneration = useRef(0)
  const [accounts, setAccounts] = useState<PublishAccount[]>([])
  const [records, setRecords] = useState<PublishRecord[]>([])
  const [statsById, setStatsById] = useState<Record<string, VideoStats>>({})
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const [result, setResult] = useState<PublishSubmitResult | null>(null)

  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  const loadAccounts = useCallback(async () => {
    if (!isTauriRuntime()) return
    try {
      const items = await api.publishListAccounts()
      if (alive.current) setAccounts(items)
    } catch (caught) {
      if (alive.current) setError(errorText(caught))
    }
  }, [])

  const loadRecords = useCallback(async () => {
    if (!isTauriRuntime()) return
    try {
      const items = await api.publishListRecords()
      if (alive.current) setRecords(items)
    } catch (caught) {
      if (alive.current) setError(errorText(caught))
    }
  }, [])

  const submit = useCallback(async (request: PublishRequest) => {
    const ticket = ++submitGeneration.current
    setPending(true)
    setError('')
    try {
      const value = await api.publishSubmit(request)
      if (!alive.current || ticket !== submitGeneration.current) return null
      setResult(value)
      setRecords((previous) => mergeRecords(previous, value.records))
      return value
    } catch (caught) {
      if (!alive.current || ticket !== submitGeneration.current) return null
      setError(errorText(caught))
      setResult(null)
      return null
    } finally {
      if (alive.current && ticket === submitGeneration.current) setPending(false)
    }
  }, [])

  const retry = useCallback(async (id: string) => {
    const ticket = ++submitGeneration.current
    setPending(true)
    setError('')
    try {
      const record = await api.publishRetry(id)
      if (!alive.current || ticket !== submitGeneration.current) return null
      setRecords((previous) => mergeRecords(previous, [record]))
      return record
    } catch (caught) {
      if (!alive.current || ticket !== submitGeneration.current) return null
      setError(errorText(caught))
      return null
    } finally {
      if (alive.current && ticket === submitGeneration.current) setPending(false)
    }
  }, [])

  const refreshRecord = useCallback(async (id: string) => {
    if (!isTauriRuntime()) return null
    try {
      const record = await api.publishRefreshRecord(id)
      if (!alive.current) return null
      setRecords((previous) => mergeRecords(previous, [record]))
      return record
    } catch (caught) {
      if (alive.current) setError(errorText(caught))
      return null
    }
  }, [])

  const loadStats = useCallback(async (id: string) => {
    try {
      const stats = await api.publishStats(id)
      if (!alive.current) return null
      setStatsById((previous) => ({ ...previous, [id]: stats }))
      return stats
    } catch (caught) {
      if (alive.current) setError(errorText(caught))
      return null
    }
  }, [])

  return { accounts, records, statsById, pending, error, result, loadAccounts, loadRecords, submit, retry, refreshRecord, loadStats, setError }
}
