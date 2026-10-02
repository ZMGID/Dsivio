import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import type { MetricRange, ShopMetrics } from '../../../generated/commerce'

const commerceMetrics = vi.fn()

vi.mock('../../../api/tauri', () => ({
  api: {
    commerceMetrics: (...args: unknown[]) => commerceMetrics(...args),
  },
}))

import { useCommerce } from './useCommerce'

function deferred<T>() {
  let resolve: (value: T) => void = () => {}
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

function metrics(shopId: string, range: MetricRange): ShopMetrics {
  return {
    shopId,
    range,
    currency: 'SGD',
    values: { orders: 1 },
    unsupported: [],
    fetchedAt: '2026-10-02T00:00:00+00:00',
  }
}

beforeEach(() => {
  commerceMetrics.mockReset()
})

it('discards a late metrics result when the shop changes', async () => {
  const first = deferred<ShopMetrics>()
  const second = deferred<ShopMetrics>()
  commerceMetrics.mockImplementation((shopId: string) => (shopId === 'shop-a' ? first.promise : second.promise))
  const { result, rerender } = renderHook(
    ({ shopId, range }: { shopId: string; range: MetricRange }) => useCommerce(shopId, range),
    { initialProps: { shopId: 'shop-a', range: 'today' } },
  )
  rerender({ shopId: 'shop-b', range: 'today' })
  await act(async () => {
    first.resolve(metrics('shop-a', 'today'))
    second.resolve(metrics('shop-b', 'today'))
  })
  await waitFor(() => expect(result.current.metrics?.shopId).toBe('shop-b'))
  expect(result.current.metrics?.range).toBe('today')
  expect(result.current.error).toBe('')
})

it('discards a late metrics result when the range changes', async () => {
  const first = deferred<ShopMetrics>()
  const second = deferred<ShopMetrics>()
  commerceMetrics.mockImplementation((_shopId: string, range: MetricRange) => (range === 'today' ? first.promise : second.promise))
  const { result, rerender } = renderHook(
    ({ shopId, range }: { shopId: string; range: MetricRange }) => useCommerce(shopId, range),
    { initialProps: { shopId: 'shop-a', range: 'today' } },
  )
  rerender({ shopId: 'shop-a', range: 'last7' })
  await act(async () => {
    first.resolve(metrics('shop-a', 'today'))
    second.resolve(metrics('shop-a', 'last7'))
  })
  await waitFor(() => expect(result.current.metrics?.range).toBe('last7'))
  expect(result.current.metrics?.shopId).toBe('shop-a')
})
