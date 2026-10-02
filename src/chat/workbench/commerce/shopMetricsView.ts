import type { MetricKey, ShopMetrics } from '../../../generated/commerce'

export const METRIC_KEYS: { key: MetricKey; money: boolean }[] = [
  { key: 'gmv', money: true },
  { key: 'orders', money: false },
  { key: 'refundAmount', money: true },
  { key: 'refundOrders', money: false },
  { key: 'buyers', money: false },
  { key: 'productsLive', money: false },
  { key: 'pendingShipment', money: false },
]

export type ShopMetricResult =
  | { state: 'ready'; metrics: ShopMetrics }
  | { state: 'error'; message: string }

export type DisplayCell =
  | { kind: 'value'; text: string; amount: number; currency?: string }
  | { kind: 'unsupported' }
  | { kind: 'error'; message: string }
  | { kind: 'mixed' }
  | { kind: 'empty' }

export function errorText(error: unknown): string {
  if (typeof error === 'string' && error.trim()) return error
  if (error instanceof Error && error.message.trim()) return error.message
  return '请求失败'
}

function formatAmount(amount: number): string {
  if (Number.isInteger(amount)) return String(amount)
  const rounded = Math.round(amount * 100) / 100
  return String(rounded)
}

export function formatMetric(amount: number, money: boolean, currency?: string | null): string {
  const text = formatAmount(amount)
  if (!money || !currency) return text
  return `${text} ${currency}`
}

export function metricCell(result: ShopMetricResult | undefined, key: MetricKey, money: boolean): DisplayCell {
  if (!result) return { kind: 'empty' }
  if (result.state === 'error') return { kind: 'error', message: result.message }
  const { metrics } = result
  if (metrics.unsupported.includes(key)) return { kind: 'unsupported' }
  const amount = metrics.values[key]
  if (typeof amount !== 'number' || !Number.isFinite(amount)) {
    return metrics.error ? { kind: 'error', message: metrics.error } : { kind: 'empty' }
  }
  const currency = metrics.currency || undefined
  return { kind: 'value', text: formatMetric(amount, money, currency), amount, currency }
}

/** Same-currency sums only. Unsupported and errors stay explicit and are not treated as zero. */
export function summarizeMetric(results: Array<ShopMetricResult | undefined>, key: MetricKey, money: boolean): DisplayCell {
  if (results.length === 0) return { kind: 'empty' }
  const cells = results.map((result) => metricCell(result, key, money))
  const error = cells.find((cell) => cell.kind === 'error')
  if (error) return error
  if (cells.some((cell) => cell.kind !== 'value')) {
    return cells.some((cell) => cell.kind === 'unsupported') ? { kind: 'unsupported' } : { kind: 'empty' }
  }
  const values = cells.filter((cell): cell is Extract<DisplayCell, { kind: 'value' }> => cell.kind === 'value')
  if (money) {
    if (values.length > 1 && values.some((cell) => !cell.currency)) return { kind: 'mixed' }
    const currencies = new Set(values.map((cell) => cell.currency))
    if (currencies.size > 1) return { kind: 'mixed' }
    const amount = Math.round(values.reduce((sum, cell) => sum + cell.amount, 0) * 100) / 100
    const currency = values[0]?.currency
    return { kind: 'value', text: formatMetric(amount, true, currency), amount, currency }
  }
  const amount = values.reduce((sum, cell) => sum + cell.amount, 0)
  return { kind: 'value', text: formatMetric(amount, false), amount }
}
