import { useEffect, useState } from 'react'
import { api } from '../../../api/tauri'
import type { MetricRange, ShopMetrics } from '../../../generated/commerce'

export interface CommerceMetricsState {
  metrics: ShopMetrics | null
  loading: boolean
  error: string
}

/**
 * Loads shop metrics for the selected shop and range.
 * A response that arrives after the shop or range has changed is discarded.
 */
export function useCommerce(shopId: string | null, range: MetricRange): CommerceMetricsState {
  const [metrics, setMetrics] = useState<ShopMetrics | null>(null)
  const [loading, setLoading] = useState(Boolean(shopId))
  const [error, setError] = useState('')

  useEffect(() => {
    if (!shopId) {
      setMetrics(null)
      setLoading(false)
      setError('')
      return
    }
    let active = true
    setLoading(true)
    setError('')
    void api.commerceMetrics(shopId, range).then(
      (loaded) => {
        if (!active) return
        setMetrics(loaded)
        setLoading(false)
      },
      (failure: unknown) => {
        if (!active) return
        setMetrics(null)
        setError(failure instanceof Error ? failure.message : String(failure))
        setLoading(false)
      },
    )
    return () => {
      active = false
    }
  }, [shopId, range])

  return { metrics, loading, error }
}
