import type { ProductSaveRequest, ProductSku } from '../../../generated/products'

export type ProductSkuDraft = {
  code: string
  name: string
  price: string
  stock: string
}

/** Form text before it becomes a `ProductSaveRequest`. Image paths are local files. */
export type ProductFormDraft = {
  id?: string
  revision?: number
  name: string
  description: string
  currency: string
  price: string
  stock: string
  skus: ProductSkuDraft[]
  images: string[]
}

export type ProductFormError = 'price' | 'stock' | 'sku-price' | 'sku-stock' | 'image'

function parseNumber(raw: string): number | null | 'bad' {
  const text = raw.trim()
  if (!text) return null
  if (!/^\d+(\.\d+)?$/.test(text)) return 'bad'
  const value = Number(text)
  return Number.isFinite(value) ? value : 'bad'
}

function parseStock(raw: string): number | null | 'bad' {
  const text = raw.trim()
  if (!text) return null
  if (!/^\d+$/.test(text)) return 'bad'
  const value = Number(text)
  return Number.isSafeInteger(value) ? value : 'bad'
}

/** Assemble one save. Empty SKU rows are omitted. Invalid numbers and blob URLs do not produce a request. */
export function buildProductSaveRequest(
  draft: ProductFormDraft,
): { ok: true; request: ProductSaveRequest } | { ok: false; error: ProductFormError } {
  if (draft.images.some((path) => {
    const lower = path.trim().toLowerCase()
    return lower.startsWith('blob:') || lower.startsWith('data:') || lower.includes('://')
  })) return { ok: false, error: 'image' }
  const price = parseNumber(draft.price)
  if (price === 'bad') return { ok: false, error: 'price' }
  const stock = parseStock(draft.stock)
  if (stock === 'bad') return { ok: false, error: 'stock' }
  const skus: ProductSku[] = []
  for (const sku of draft.skus) {
    const code = sku.code.trim()
    const name = sku.name.trim()
    const skuPrice = parseNumber(sku.price)
    if (skuPrice === 'bad') return { ok: false, error: 'sku-price' }
    const skuStock = parseStock(sku.stock)
    if (skuStock === 'bad') return { ok: false, error: 'sku-stock' }
    if (!code && !name && skuPrice == null && skuStock == null) continue
    skus.push({ code, name, price: skuPrice, stock: skuStock })
  }
  const currency = draft.currency.trim()
  return {
    ok: true,
    request: {
      id: draft.id ?? null,
      revision: draft.revision ?? null,
      name: draft.name,
      description: draft.description,
      currency: currency ? currency : null,
      price,
      stock,
      skus,
      images: draft.images,
      inlineImages: [],
    },
  }
}
