import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../../api/tauri'
import type { Product, ProductSaveRequest } from '../../../generated/products'

/**
 * Saved products. A newer load or unmount drops a late list/save so reopening
 * shows the store, not a response from the previous visit.
 */
export function useProducts() {
  const [products, setProducts] = useState<Product[]>([])
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
      const next = await api.productsList()
      if (!mounted.current || gen !== generation.current) return next
      setProducts(next)
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

  const save = useCallback(async (request: ProductSaveRequest) => {
    const gen = ++generation.current
    const saved = await api.productsSave(request)
    if (!mounted.current || gen !== generation.current) return saved
    setProducts((current) => {
      const rest = current.filter((product) => product.id !== saved.id)
      return [saved, ...rest].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id))
    })
    setError(null)
    return saved
  }, [])

  const remove = useCallback(async (id: string) => {
    const gen = ++generation.current
    await api.productsDelete(id)
    if (!mounted.current || gen !== generation.current) return
    setProducts((current) => current.filter((product) => product.id !== id))
  }, [])

  return { products, loading, error, refresh, save, remove }
}
