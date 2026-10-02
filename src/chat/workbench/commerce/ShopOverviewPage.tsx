import { useState } from 'react'
import { RefreshCw, Store } from 'lucide-react'
import type { ShopPlatform } from '../../../api/tauri'
import { IconButton } from '../../../components/Button'
import { useT, type I18n } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { ALL_SHOP_PLATFORMS } from './shopPlatforms'
import { METRIC_KEYS, metricCell, summarizeMetric, type DisplayCell } from './shopMetricsView'
import { useShopOverview } from './useShopOverview'
import './shopOverview.css'

const METRIC_LABEL: Record<typeof METRIC_KEYS[number]['key'], keyof I18n> = {
  gmv: 'workbenchKpiGmv',
  orders: 'workbenchKpiOrders',
  refundAmount: 'workbenchKpiRefund',
  refundOrders: 'workbenchMetricRefundOrders',
  buyers: 'workbenchMetricBuyers',
  productsLive: 'workbenchMetricProductsLive',
  pendingShipment: 'workbenchMetricPendingShip',
}

function cellText(t: I18n, cell: DisplayCell): string {
  if (cell.kind === 'value') return cell.text
  if (cell.kind === 'unsupported') return t.workbenchMetricUnsupported
  if (cell.kind === 'error') return cell.message
  if (cell.kind === 'mixed') return t.workbenchMetricMixed
  return t.workbenchMetricEmpty
}

/**
 * Bound shops and per-shop metrics for today / yesterday / 7 / 30 days.
 * The hook owns the requests; switching range drops a late response.
 */
export function ShopOverviewPage() {
  const t = useT()
  const { range, setRange, shops, results, error, refresh } = useShopOverview()
  const [platform, setPlatform] = useState<ShopPlatform | 'all'>('all')
  const visible = platform === 'all' ? shops : shops.filter((shop) => shop.platform === platform)
  const ranges = [
    { id: 'today' as const, label: t.workbenchRangeToday },
    { id: 'yesterday' as const, label: t.workbenchRangeYesterday },
    { id: 'last7' as const, label: t.workbenchRange7d },
    { id: 'last30' as const, label: t.workbenchRange30d },
  ]

  return (
    <WorkbenchPage
      className="shop-overview-page"
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavOverview}
      error={error}
      actions={(
        <>
          {ranges.map((item) => (
            // ui-guard-ignore:raw-primitive -- 时间范围是分段选择控件，沿用工作台 chip 样式。
            <button
              key={item.id}
              type="button"
              className={`workbench-chip${range === item.id ? ' is-active' : ''}`}
              aria-pressed={range === item.id}
              onClick={() => setRange(item.id)}
            >
              {item.label}
            </button>
          ))}
          <IconButton label={t.workbenchRefresh} size="sm" onClick={refresh}>
            <RefreshCw size={14} />
          </IconButton>
        </>
      )}
    >
      <div className="workbench-kpi-grid">
        {METRIC_KEYS.map((item) => (
          <div key={item.key} className="workbench-kpi">
            <span className="workbench-kpi-value">{cellText(t, summarizeMetric(visible.map((shop) => results[shop.id]), item.key, item.money))}</span>
            <span className="workbench-kpi-label">{t[METRIC_LABEL[item.key]]}</span>
          </div>
        ))}
      </div>

      <WorkbenchCard title={t.workbenchOverviewDetail} hint={t.workbenchOverviewDetailHint}>
        <div className="workbench-tabs">
          {/* ui-guard-ignore:raw-primitive -- 平台筛选沿用工作台 tab。 */}
          <button type="button" className={`workbench-tab${platform === 'all' ? ' is-active' : ''}`} aria-pressed={platform === 'all'} onClick={() => setPlatform('all')}>
            {t.workbenchFilterAll}<span className="workbench-tab-count">{shops.length}</span>
          </button>
          {ALL_SHOP_PLATFORMS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${platform === item.id ? ' is-active' : ''}`}
              aria-pressed={platform === item.id}
              onClick={() => setPlatform(item.id)}
            >
              {item.name}<span className="workbench-tab-count">{shops.filter((shop) => shop.platform === item.id).length}</span>
            </button>
          ))}
        </div>
        {visible.length === 0 ? (
          <WorkbenchEmpty compact icon={<Store size={24} />} title={t.workbenchOverviewEmpty}>
            {t.workbenchOverviewDetailHint}
          </WorkbenchEmpty>
        ) : (
          <div className="custom-scrollbar workbench-table-scroll">
            <table className="workbench-table">
              <thead>
                <tr>
                  <th>{t.workbenchOverviewColShop}</th>
                  {METRIC_KEYS.map((item) => <th key={item.key}>{t[METRIC_LABEL[item.key]]}</th>)}
                </tr>
              </thead>
              <tbody>
                {visible.map((shop) => {
                  const result = results[shop.id]
                  return (
                    <tr key={shop.id}>
                      <td>
                        {shop.name}
                        {result?.state === 'ready' && result.metrics.error ? <div>{result.metrics.error}</div> : null}
                      </td>
                      {result?.state === 'error' ? (
                        <td colSpan={METRIC_KEYS.length}>{result.message}</td>
                      ) : METRIC_KEYS.map((item) => (
                        <td key={item.key}>{cellText(t, metricCell(result, item.key, item.money))}</td>
                      ))}
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
