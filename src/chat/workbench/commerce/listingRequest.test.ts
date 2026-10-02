import { expect, it } from 'vitest'
import type { Product } from '../../../generated/products'
import { applyProduct, buildListingRequest, canRetryListing, emptyListingForm, type ListingFormState } from './listingRequest'

function form(partial: Partial<ListingFormState> = {}): ListingFormState {
  return {
    ...emptyListingForm(),
    title: '帆布鞋',
    description: '舒适',
    price: '19.9',
    currency: 'sgd',
    stock: '4',
    sku: 'SKU-1',
    images: ['/tmp/shoe.png'],
    weightKg: '0.4',
    lengthCm: '30',
    widthCm: '20',
    heightCm: '10',
    shopIds: ['s1'],
    categories: { s1: '100' },
    attributes: { s1: { '9': { value: '帆布', values: [] } } },
    ...partial,
  }
}

const shops = [
  { id: 's1', name: '虾皮一店', listing: true },
  { id: 's2', name: '虾皮二店', listing: true },
]

const defs = {
  s1: [{ id: '9', name: '材质', required: true, input: 'text' as const, options: [] }],
  s2: [{ id: '9', name: '材质', required: true, input: 'text' as const, options: [] }],
}

it('builds one target per shop and does not copy a category across shops', () => {
  const missing = buildListingRequest(form({ shopIds: ['s1', 's2'], categories: { s1: '100' }, attributes: { s1: { '9': { value: '帆布', values: [] } } } }), shops, defs)
  expect(missing.ok).toBe(false)
  if (!missing.ok) expect(missing.errors).toContain('请为虾皮二店选择类目')

  const built = buildListingRequest(form({
    shopIds: ['s1', 's2'],
    categories: { s1: '100', s2: '200' },
    attributes: {
      s1: { '9': { value: '帆布', values: [] } },
      s2: { '9': { value: '尼龙', values: [] } },
    },
  }), shops, defs)
  expect(built.ok).toBe(true)
  if (!built.ok) return
  expect(built.draft).toMatchObject({ title: '帆布鞋', currency: 'SGD', price: 19.9, stock: 4, weightKg: 0.4, images: ['/tmp/shoe.png'] })
  expect(built.draft).not.toHaveProperty('productId')
  expect(built.targets).toEqual([
    { shopId: 's1', categoryId: '100', attributes: [{ id: '9', value: '帆布', values: [] }] },
    { shopId: 's2', categoryId: '200', attributes: [{ id: '9', value: '尼龙', values: [] }] },
  ])
})

it('keeps a saved select attribute when the record stores the option name', () => {
  const selectDefs = {
    s1: [{ id: '9', name: '材质', required: true, input: 'select' as const, options: [{ id: '1', name: '帆布' }] }],
  }
  const built = buildListingRequest(form({ attributes: { s1: { '9': { value: '帆布', values: [] } } } }), shops, selectDefs)
  expect(built.ok).toBe(true)
  if (built.ok) expect(built.targets[0].attributes).toEqual([{ id: '9', value: '帆布', values: [] }])
})

it('rejects a relative image and a shop the platform cannot list', () => {
  const relative = buildListingRequest(form({ images: ['shoe.png'] }), shops, defs)
  expect(relative.ok).toBe(false)
  if (!relative.ok) expect(relative.errors.some((error) => error.includes('绝对路径'))).toBe(true)
  const blocked = buildListingRequest(form({ shopIds: ['s3'] }), [{ id: 's3', name: '希音店', listing: false, note: '该平台尚未接入上架' }], {})
  expect(blocked.ok).toBe(false)
  if (!blocked.ok) expect(blocked.errors).toContain('该平台尚未接入上架')
})

it('retries only failed and rejected listings', () => {
  expect(canRetryListing('failed')).toBe(true)
  expect(canRetryListing('rejected')).toBe(true)
  for (const status of ['submitting', 'reviewing', 'live', 'uncertain', 'banned'] as const) {
    expect(canRetryListing(status)).toBe(false)
  }
})

it('prefills an archive without choosing a category', () => {
  const product: Product = {
    id: 'p1', revision: 1, name: '档案鞋', description: '档案描述', currency: 'USD', price: 8, stock: 2,
    skus: [{ code: 'ARC', name: 'ARC', price: 8, stock: 2 }], images: ['/tmp/from-archive.png'],
    createdAt: '2026-10-02T00:00:00Z', updatedAt: '2026-10-02T00:00:00Z',
  }
  const next = applyProduct(emptyListingForm(), product)
  expect(next.title).toBe('档案鞋')
  expect(next.categories).toEqual({})
  expect(next.shopIds).toEqual([])
  expect(next.images).toEqual(['/tmp/from-archive.png'])
})
