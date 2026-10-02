import { expect, it } from 'vitest'
import { buildProductSaveRequest, type ProductFormDraft } from './productForm'

function draft(partial: Partial<ProductFormDraft> = {}): ProductFormDraft {
  return {
    name: '运动水杯',
    description: '轻便，耐磨',
    currency: 'cny',
    price: '19.5',
    stock: '8',
    skus: [{ code: ' RED-1 ', name: '红色', price: '19.5', stock: '3' }],
    images: ['/tmp/cup.png'],
    ...partial,
  }
}

it('builds a save request from the form', () => {
  const built = buildProductSaveRequest(draft())
  expect(built.ok).toBe(true)
  if (!built.ok) return
  expect(built.request).toEqual({
    id: null,
    revision: null,
    name: '运动水杯',
    description: '轻便，耐磨',
    currency: 'cny',
    price: 19.5,
    stock: 8,
    skus: [{ code: 'RED-1', name: '红色', price: 19.5, stock: 3 }],
    images: ['/tmp/cup.png'],
    inlineImages: [],
  })
})

it('keeps the revision when editing and drops a blank sku row', () => {
  const built = buildProductSaveRequest(draft({
    id: 'p-1',
    revision: 2,
    price: '',
    stock: '',
    currency: '  ',
    skus: [
      { code: '', name: '', price: '', stock: '' },
      { code: 'BLU', name: '', price: '', stock: '' },
    ],
    images: [],
  }))
  expect(built.ok).toBe(true)
  if (!built.ok) return
  expect(built.request.id).toBe('p-1')
  expect(built.request.revision).toBe(2)
  expect(built.request.price).toBeNull()
  expect(built.request.stock).toBeNull()
  expect(built.request.currency).toBeNull()
  expect(built.request.skus).toEqual([{ code: 'BLU', name: '', price: null, stock: null }])
})

it('refuses a bad price and a browser blob url', () => {
  expect(buildProductSaveRequest(draft({ price: '-1' })).ok).toBe(false)
  expect(buildProductSaveRequest(draft({ price: 'abc' })).ok).toBe(false)
  expect(buildProductSaveRequest(draft({ stock: '1.5' })).ok).toBe(false)
  const blob = buildProductSaveRequest(draft({ images: ['blob:http://localhost/7c9e'] }))
  expect(blob.ok).toBe(false)
  if (!blob.ok) expect(blob.error).toBe('image')
})
