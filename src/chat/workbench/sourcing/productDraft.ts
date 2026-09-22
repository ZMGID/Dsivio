import type { PickDraft, SourcingProduct } from '../../../generated/sourcing'

export function productDraft(product: SourcingProduct): PickDraft {
  return { source: 'alibaba1688', sourceId: `${product.id}:${product.skuId ?? ''}`, title: product.title,
    url: product.url, imageUrl: product.imageUrl, price: product.price, currency: 'CNY', supplier: product.supplier,
    tags: [], note: product.skuTitle ?? '', stage: 'new' }
}
