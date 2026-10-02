import { useEffect, useRef, useState } from 'react'
import { api, type ShopConnection } from '../../../api/tauri'
import type { MetricRange } from '../../../generated/commerce'
import { errorText, type ShopMetricResult } from './shopMetricsView'

/**
 * Bound shops and one metrics request per shop for the selected range.
 * A newer range, refresh, or unmount drops the previous response.
 */
export function useShopOverview() {
  const [range, setRange] = useState<MetricRange>('today')
  const [reload, setReload] = useState(0)
  const [shops, setShops] = useState<ShopConnection[]>([])
  const [results, setResults] = useState<Record<string, ShopMetricResult>>({})
  const [error, setError] = useState('')
  const generation = useRef(0)

  useEffect(() => {
    const gen = ++generation.current
    setResults({})
    setError('')
    ;(async () => {
      let items: ShopConnection[]
      try {
        items = await api.commerceShops()
      } catch (err) {
        if (gen !== generation.current) return
        setShops([])
        setError(errorText(err))
        return
      }
      if (gen !== generation.current) return
      setShops(items)
      setError('')
      const entries = await Promise.all(items.map(async (shop) => {
        try {
          const metrics = await api.commerceMetrics(shop.id, range)
          return [shop.id, { state: 'ready' as const, metrics }] as const
        } catch (err) {
          return [shop.id, { state: 'error' as const, message: errorText(err) }] as const
        }
      }))
      if (gen !== generation.current) return
      setResults(Object.fromEntries(entries))
    })()
    return () => { generation.current += 1 }
  }, [range, reload])

  return { range, setRange, shops, results, error, refresh: () => setReload((value) => value + 1) }
}
