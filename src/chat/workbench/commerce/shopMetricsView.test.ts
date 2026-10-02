import { expect, it } from 'vitest'
import type { ShopMetrics } from '../../../generated/commerce'
import { metricCell, summarizeMetric, type ShopMetricResult } from './shopMetricsView'

function ready(partial: Partial<ShopMetrics>): ShopMetricResult {
  return {
    state: 'ready',
    metrics: {
      shopId: 's1',
      range: 'today',
      currency: 'SGD',
      values: { gmv: 10, orders: 2, refundAmount: 1, refundOrders: 1, buyers: 2, productsLive: 3, pendingShipment: 1 },
      unsupported: [],
      fetchedAt: '2026-10-02T00:00:00Z',
      ...partial,
    },
  }
}

it('shows a real zero and hides unsupported zeros', () => {
  const result = ready({ values: { gmv: 10, orders: 0, refundAmount: 0 }, unsupported: ['refundAmount'], currency: 'SGD' })
  expect(metricCell(result, 'orders', false)).toMatchObject({ kind: 'value', text: '0' })
  expect(metricCell(result, 'refundAmount', true)).toEqual({ kind: 'unsupported' })
  expect(metricCell(result, 'buyers', false)).toEqual({ kind: 'empty' })
})

it('shows a fetch error instead of zero', () => {
  const failed: ShopMetricResult = { state: 'error', message: '店铺授权已失效' }
  expect(metricCell(failed, 'gmv', true)).toEqual({ kind: 'error', message: '店铺授权已失效' })
  expect(summarizeMetric([failed, ready({})], 'gmv', true)).toEqual({ kind: 'error', message: '店铺授权已失效' })
})

it('sums one currency and refuses to add different currencies', () => {
  const sg = ready({ currency: 'SGD', values: { gmv: 10, orders: 2 } })
  const br = ready({ shopId: 's2', currency: 'BRL', values: { gmv: 20, orders: 3 } })
  const more = ready({ shopId: 's3', currency: 'SGD', values: { gmv: 5, orders: 1 } })
  expect(summarizeMetric([sg, more], 'gmv', true)).toMatchObject({ kind: 'value', text: '15 SGD', amount: 15 })
  expect(summarizeMetric([sg, more], 'orders', false)).toMatchObject({ kind: 'value', amount: 3 })
  expect(summarizeMetric([sg, br], 'gmv', true)).toEqual({ kind: 'mixed' })
  expect(summarizeMetric([sg, ready({ unsupported: ['gmv'] })], 'gmv', true)).toEqual({ kind: 'unsupported' })
})
