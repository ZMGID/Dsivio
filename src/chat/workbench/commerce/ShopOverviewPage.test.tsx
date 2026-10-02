import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api, type ShopConnection } from '../../../api/tauri'
import type { MetricRange, ShopMetrics } from '../../../generated/commerce'
import { ShopOverviewPage } from './ShopOverviewPage'

vi.mock('../../../api/tauri', () => ({
  api: { commerceShops: vi.fn(), commerceMetrics: vi.fn() },
}))

function shop(partial: Partial<ShopConnection> = {}): ShopConnection {
  return {
    id: 's1', platform: 'shopee', remoteId: 'r1', name: '新加坡店', region: 'SG',
    boundAt: '2026-10-01T00:00:00Z', checkedAt: '2026-10-02T00:00:00Z', status: 'connected', detail: null,
    ...partial,
  }
}

function metrics(partial: Partial<ShopMetrics> = {}): ShopMetrics {
  return {
    shopId: 's1', range: 'today', currency: 'SGD',
    values: { gmv: 10, orders: 2, refundAmount: 1, refundOrders: 1, buyers: 2, productsLive: 3, pendingShipment: 1 },
    unsupported: [], fetchedAt: '2026-10-02T00:00:00Z',
    ...partial,
  }
}

function kpi(label: string): HTMLElement {
  const node = screen.getAllByText(label).map((item) => item.closest('.workbench-kpi')).find((item): item is HTMLElement => item != null)
  if (!node) throw new Error(label)
  return node
}

beforeEach(() => {
  vi.mocked(api.commerceShops).mockReset()
  vi.mocked(api.commerceMetrics).mockReset()
  vi.mocked(api.commerceShops).mockResolvedValue([])
  vi.mocked(api.commerceMetrics).mockResolvedValue(metrics())
})

it('shows supported values, a true zero, and 平台不支持 instead of an unsupported zero', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([shop()])
  vi.mocked(api.commerceMetrics).mockResolvedValue(metrics({
    values: { gmv: 10, orders: 0, refundAmount: 0, refundOrders: 1, buyers: 2, productsLive: 3, pendingShipment: 1 },
    unsupported: ['refundAmount'],
  }))
  render(<ShopOverviewPage />)
  expect(await screen.findByRole('cell', { name: '10 SGD' })).toBeInTheDocument()
  expect(screen.getByRole('cell', { name: '0' })).toBeInTheDocument()
  expect(screen.getAllByRole('cell', { name: '平台不支持' }).length).toBeGreaterThan(0)
  expect(kpi('退款金额')).toHaveTextContent('平台不支持')
  expect(kpi('退款金额')).not.toHaveTextContent('0')
  expect(screen.queryByText('¥0.00')).toBeNull()
})

it('shows a shop error instead of zeros', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([shop({ platform: 'shein', name: '希音店' })])
  vi.mocked(api.commerceMetrics).mockRejectedValue(new Error('该平台尚未接入店铺指标'))
  render(<ShopOverviewPage />)
  expect(await screen.findByRole('cell', { name: '该平台尚未接入店铺指标' })).toBeInTheDocument()
  expect(screen.queryByRole('cell', { name: '0' })).toBeNull()
  expect(screen.queryByText('¥0.00')).toBeNull()
  expect(kpi('成交金额')).toHaveTextContent('该平台尚未接入店铺指标')
})

it('does not add GMV across currencies', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([
    shop({ id: 'sg', name: '新加坡店' }),
    shop({ id: 'br', name: '巴西店', region: 'BR' }),
  ])
  vi.mocked(api.commerceMetrics).mockImplementation(async (id: string) => metrics({
    shopId: id,
    currency: id === 'br' ? 'BRL' : 'SGD',
    values: { gmv: id === 'br' ? 20 : 10, orders: 1, refundAmount: 1, refundOrders: 1, buyers: 1, productsLive: 1, pendingShipment: 1 },
  }))
  render(<ShopOverviewPage />)
  expect(await screen.findByRole('cell', { name: '10 SGD' })).toBeInTheDocument()
  expect(screen.getByRole('cell', { name: '20 BRL' })).toBeInTheDocument()
  expect(kpi('成交金额')).toHaveTextContent('多币种，不合计')
  expect(screen.queryByText('30')).toBeNull()
  expect(screen.queryByText('30 SGD')).toBeNull()
})

it('shows the orders-fetch error and labels those zeroed keys unsupported', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([shop()])
  vi.mocked(api.commerceMetrics).mockResolvedValue(metrics({
    values: { gmv: 0, orders: 0, refundAmount: 3, refundOrders: 1, buyers: 0, productsLive: 4, pendingShipment: 0 },
    unsupported: ['gmv', 'orders', 'buyers', 'pendingShipment'],
    error: '订单读取失败',
  }))
  render(<ShopOverviewPage />)
  expect(await screen.findByText('订单读取失败')).toBeInTheDocument()
  expect(screen.getByRole('cell', { name: '3 SGD' })).toBeInTheDocument()
  expect(screen.getAllByRole('cell', { name: '平台不支持' })).toHaveLength(4)
  expect(screen.queryByRole('cell', { name: '0' })).toBeNull()
})

it('drops a late range result after the shopper switches range', async () => {
  let resolveToday: (value: ShopMetrics) => void = () => {}
  vi.mocked(api.commerceShops).mockResolvedValue([shop()])
  vi.mocked(api.commerceMetrics).mockImplementation((_id: string, range: MetricRange) => {
    if (range === 'today') return new Promise((resolve) => { resolveToday = resolve })
    return Promise.resolve(metrics({ range, values: { gmv: 7, orders: 1, refundAmount: 1, refundOrders: 1, buyers: 1, productsLive: 1, pendingShipment: 1 } }))
  })
  render(<ShopOverviewPage />)
  await waitFor(() => expect(api.commerceMetrics).toHaveBeenCalledWith('s1', 'today'))
  fireEvent.click(screen.getByRole('button', { name: '昨天' }))
  expect(await screen.findByRole('cell', { name: '7 SGD' })).toBeInTheDocument()
  await act(async () => { resolveToday(metrics({ values: { gmv: 99, orders: 1, refundAmount: 1, refundOrders: 1, buyers: 1, productsLive: 1, pendingShipment: 1 } })) })
  expect(screen.getByRole('cell', { name: '7 SGD' })).toBeInTheDocument()
  expect(screen.queryByRole('cell', { name: '99 SGD' })).toBeNull()
})

it('drops a late shop list after refresh and loads again after reopen', async () => {
  let resolveFirst: (value: ShopConnection[]) => void = () => {}
  vi.mocked(api.commerceShops).mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve }))
  const view = render(<ShopOverviewPage />)
  await waitFor(() => expect(api.commerceShops).toHaveBeenCalledTimes(1))
  vi.mocked(api.commerceShops).mockResolvedValueOnce([shop({ id: 'new', name: '新店' })])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('cell', { name: '新店' })).toBeInTheDocument()
  await act(async () => { resolveFirst([shop({ name: '旧店' })]) })
  expect(screen.queryByRole('cell', { name: '旧店' })).toBeNull()

  view.unmount()
  vi.mocked(api.commerceShops).mockResolvedValue([shop({ id: 'back', name: '回来的店' })])
  render(<ShopOverviewPage />)
  expect(await screen.findByRole('cell', { name: '回来的店' })).toBeInTheDocument()
  expect(screen.queryByRole('cell', { name: '新店' })).toBeNull()
})
