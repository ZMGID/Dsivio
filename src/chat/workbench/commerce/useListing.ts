import { useCallback, useEffect, useRef, useState } from 'react'
import { api, type ShopConnection } from '../../../api/tauri'
import type { Category, CategoryAttribute, CommerceCapabilities, ListingDraft, ListingRecord, ListingTarget } from '../../../generated/commerce'
import type { Product } from '../../../generated/products'
import { canRetryListing } from './listingRequest'
import { errorText } from './shopMetricsView'

export type CapabilityState =
  | { state: 'ready'; value: CommerceCapabilities }
  | { state: 'error'; message: string }

export type CategoryState = {
  parentId: string
  items: Category[]
  error: string
  loading: boolean
}

export type AttributeState = {
  categoryId: string
  items: CategoryAttribute[]
  error: string
  loading: boolean
}

function mergeRecords(current: ListingRecord[], incoming: ListingRecord[]): ListingRecord[] {
  const map = new Map(current.map((record) => [record.id, record]))
  for (const record of incoming) map.set(record.id, record)
  return [...map.values()].sort((a, b) => b.createdAt.localeCompare(a.createdAt))
}

/**
 * Shops, capabilities, categories, attributes, and listing records.
 * Requests live here; the page does not poll.
 */
export function useListing() {
  const [reloadShops, setReloadShops] = useState(0)
  const [shops, setShops] = useState<ShopConnection[]>([])
  const [shopError, setShopError] = useState('')
  const [capabilities, setCapabilities] = useState<Record<string, CapabilityState>>({})
  const [categories, setCategories] = useState<Record<string, CategoryState>>({})
  const [attributes, setAttributes] = useState<Record<string, AttributeState>>({})
  const [products, setProducts] = useState<Product[]>([])
  const [productError, setProductError] = useState('')
  const [records, setRecords] = useState<ListingRecord[]>([])
  const [listError, setListError] = useState('')
  const shopGen = useRef(0)
  const listGen = useRef(0)
  const categoryGen = useRef<Record<string, number>>({})
  const attributeGen = useRef<Record<string, number>>({})

  useEffect(() => {
    const gen = ++shopGen.current
    setShopError('')
    ;(async () => {
      let items: ShopConnection[]
      try {
        items = await api.commerceShops()
      } catch (err) {
        if (gen !== shopGen.current) return
        setShopError(errorText(err))
        setShops([])
        return
      }
      if (gen !== shopGen.current) return
      setShops(items)
      setShopError('')
      const caps = await Promise.all(items.map(async (shop) => {
        try {
          const value = await api.commerceCapabilities(shop.id)
          return [shop.id, { state: 'ready' as const, value }] as const
        } catch (err) {
          return [shop.id, { state: 'error' as const, message: errorText(err) }] as const
        }
      }))
      if (gen !== shopGen.current) return
      setCapabilities(Object.fromEntries(caps))
      try {
        const nextProducts = await api.productsList()
        if (gen !== shopGen.current) return
        setProducts(nextProducts)
        setProductError('')
      } catch (err) {
        if (gen !== shopGen.current) return
        setProducts([])
        setProductError(errorText(err))
      }
    })()
    return () => { shopGen.current += 1 }
  }, [reloadShops])

  const refreshRecords = useCallback(async () => {
    const gen = ++listGen.current
    setListError('')
    try {
      const rows = await api.commerceListings({})
      if (gen !== listGen.current) return
      setRecords(rows)
      setListError('')
    } catch (err) {
      if (gen !== listGen.current) return
      setListError(errorText(err))
    }
  }, [])

  useEffect(() => {
    void refreshRecords()
    return () => { listGen.current += 1 }
  }, [refreshRecords])

  useEffect(() => () => {
    for (const key of Object.keys(categoryGen.current)) categoryGen.current[key] += 1
    for (const key of Object.keys(attributeGen.current)) attributeGen.current[key] += 1
  }, [])

  const loadCategories = useCallback(async (shopId: string, parentId: string) => {
    const gen = (categoryGen.current[shopId] ?? 0) + 1
    categoryGen.current[shopId] = gen
    setCategories((current) => ({ ...current, [shopId]: { parentId, items: [], error: '', loading: true } }))
    try {
      const items = await api.commerceCategories(shopId, parentId)
      if (categoryGen.current[shopId] !== gen) return
      setCategories((current) => ({ ...current, [shopId]: { parentId, items, error: '', loading: false } }))
    } catch (err) {
      if (categoryGen.current[shopId] !== gen) return
      setCategories((current) => ({ ...current, [shopId]: { parentId, items: [], error: errorText(err), loading: false } }))
    }
  }, [])

  const loadAttributes = useCallback(async (shopId: string, categoryId: string) => {
    const gen = (attributeGen.current[shopId] ?? 0) + 1
    attributeGen.current[shopId] = gen
    setAttributes((current) => ({ ...current, [shopId]: { categoryId, items: [], error: '', loading: true } }))
    try {
      const items = await api.commerceAttributes(shopId, categoryId)
      if (attributeGen.current[shopId] !== gen) return
      setAttributes((current) => ({ ...current, [shopId]: { categoryId, items, error: '', loading: false } }))
    } catch (err) {
      if (attributeGen.current[shopId] !== gen) return
      setAttributes((current) => ({ ...current, [shopId]: { categoryId, items: [], error: errorText(err), loading: false } }))
    }
  }, [])

  const submit = useCallback(async (draft: ListingDraft, targets: ListingTarget[]) => {
    const gen = ++listGen.current
    const rows = await api.commerceSubmit(draft, targets, null)
    if (gen === listGen.current) {
      setRecords((current) => mergeRecords(current, rows))
      setListError('')
    }
    return rows
  }, [])

  const resubmit = useCallback(async (record: ListingRecord, draft: ListingDraft | null, target: ListingTarget | null) => {
    if (!canRetryListing(record.status)) throw new Error('只能从已拒绝或失败的记录重新提交')
    const gen = ++listGen.current
    const next = await api.commerceResubmit(record.id, draft, target)
    if (gen === listGen.current) {
      setRecords((current) => mergeRecords(current, [next]))
      setListError('')
    }
    return next
  }, [])

  const refreshRecord = useCallback(async (id: string) => {
    const gen = ++listGen.current
    const next = await api.commerceRefresh(id)
    if (gen === listGen.current) setRecords((current) => mergeRecords(current, [next]))
    return next
  }, [])

  return {
    shops, shopError, capabilities, categories, attributes, products, productError, records, listError,
    refreshShops: () => setReloadShops((value) => value + 1),
    refreshRecords, loadCategories, loadAttributes, submit, resubmit, refreshRecord,
  }
}
