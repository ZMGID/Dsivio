import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import type { Product } from '../../../generated/products'
import { ProductArchivePage } from './ProductArchivePage'

vi.mock('../../../api/tauri', () => ({
  api: { productsList: vi.fn(), productsSave: vi.fn(), productsDelete: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))

function product(partial: Partial<Product> = {}): Product {
  return {
    id: 'product-1',
    revision: 1,
    name: '运动水杯',
    description: '轻便',
    currency: 'CNY',
    price: 19.5,
    stock: 8,
    skus: [{ code: 'RED-1', name: '红色', price: 19.5, stock: 3 }],
    images: [],
    createdAt: '2026-10-02T00:00:00Z',
    updatedAt: '2026-10-02T00:00:00Z',
    ...partial,
  }
}

beforeEach(() => {
  vi.mocked(api.productsList).mockReset()
  vi.mocked(api.productsSave).mockReset()
  vi.mocked(api.productsDelete).mockReset()
  vi.mocked(api.productsList).mockResolvedValue([])
})

it('saves a product with a sku and deletes it', async () => {
  vi.mocked(api.productsSave).mockResolvedValue(product())
  render(<ProductArchivePage />)
  expect(await screen.findByText('还没有商品档案')).toBeInTheDocument()
  expect(screen.queryByText('分析商品中')).toBeNull()
  fireEvent.change(screen.getByRole('textbox', { name: '名称' }), { target: { value: '运动水杯' } })
  fireEvent.change(screen.getByRole('textbox', { name: '卖点' }), { target: { value: '轻便' } })
  fireEvent.click(screen.getByRole('button', { name: '添加 SKU' }))
  fireEvent.change(screen.getByRole('textbox', { name: 'SKU 编号' }), { target: { value: 'RED-1' } })
  fireEvent.click(screen.getByRole('button', { name: '保存商品' }))
  await waitFor(() => expect(api.productsSave).toHaveBeenCalledWith(expect.objectContaining({
    name: '运动水杯',
    description: '轻便',
    id: null,
    revision: null,
    skus: [expect.objectContaining({ code: 'RED-1' })],
  })))
  expect(await screen.findByRole('button', { name: '运动水杯' })).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '删除 运动水杯' }))
  await waitFor(() => expect(api.productsDelete).toHaveBeenCalledWith('product-1'))
  await waitFor(() => expect(screen.queryByRole('button', { name: '运动水杯' })).toBeNull())
})

it('shows a price error and saves on the next try', async () => {
  vi.mocked(api.productsSave).mockResolvedValue(product({ price: 10 }))
  render(<ProductArchivePage />)
  fireEvent.change(screen.getByRole('textbox', { name: '名称' }), { target: { value: '运动水杯' } })
  fireEvent.change(screen.getByRole('textbox', { name: '价格' }), { target: { value: 'abc' } })
  fireEvent.click(screen.getByRole('button', { name: '保存商品' }))
  expect(await screen.findByText('价格必须是非负数字')).toBeInTheDocument()
  expect(api.productsSave).not.toHaveBeenCalled()
  fireEvent.change(screen.getByRole('textbox', { name: '价格' }), { target: { value: '10' } })
  fireEvent.click(screen.getByRole('button', { name: '保存商品' }))
  await waitFor(() => expect(api.productsSave).toHaveBeenCalledWith(expect.objectContaining({ price: 10 })))
  expect(await screen.findByText('已保存')).toBeInTheDocument()
})

it('keeps the newer list when an older load resolves late', async () => {
  const first = Promise.withResolvers<Product[]>()
  vi.mocked(api.productsList).mockImplementationOnce(() => first.promise)
  render(<ProductArchivePage />)
  await waitFor(() => expect(api.productsList).toHaveBeenCalledTimes(1))
  vi.mocked(api.productsList).mockResolvedValueOnce([product({ id: 'new', name: '新商品' })])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('button', { name: '新商品' })).toBeInTheDocument()
  await act(async () => { first.resolve([product({ name: '旧商品' })]) })
  expect(screen.getByRole('button', { name: '新商品' })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: '旧商品' })).toBeNull()
})

it('loads again after leaving and reopening', async () => {
  const view = render(<ProductArchivePage />)
  await waitFor(() => expect(api.productsList).toHaveBeenCalledTimes(1))
  view.unmount()
  vi.mocked(api.productsList).mockResolvedValue([product()])
  render(<ProductArchivePage />)
  expect(await screen.findByRole('button', { name: '运动水杯' })).toBeInTheDocument()
  expect(api.productsList).toHaveBeenCalledTimes(2)
})

it('shows a revision conflict and does not pretend the save worked', async () => {
  vi.mocked(api.productsList).mockResolvedValue([product()])
  vi.mocked(api.productsSave).mockRejectedValue(new Error('此商品已在其他窗口更新，请重新打开后操作'))
  render(<ProductArchivePage />)
  fireEvent.click(await screen.findByRole('button', { name: '运动水杯' }))
  fireEvent.click(screen.getByRole('button', { name: '保存商品' }))
  expect(await screen.findByText(/重新打开/)).toBeInTheDocument()
  expect(screen.queryByText('已保存')).toBeNull()
})

it('filters the list by name and sku', async () => {
  vi.mocked(api.productsList).mockResolvedValue([
    product(),
    product({ id: 'product-2', name: '帆布袋', description: '', skus: [{ code: 'BAG', name: '大号', price: null, stock: null }] }),
  ])
  render(<ProductArchivePage />)
  expect(await screen.findByRole('button', { name: '运动水杯' })).toBeInTheDocument()
  fireEvent.change(screen.getByRole('searchbox', { name: '搜索名称、卖点或 SKU' }), { target: { value: 'bag' } })
  expect(screen.queryByRole('button', { name: '运动水杯' })).toBeNull()
  expect(screen.getByRole('button', { name: '帆布袋' })).toBeInTheDocument()
})
