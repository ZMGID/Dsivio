import { useEffect, useState } from 'react'
import { RefreshCw, Store } from 'lucide-react'
import { api, isTauriRuntime, type ShopConnection } from '../../../api/tauri'
import { IconButton } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import './shopOverview.css'
import { SHOP_PLATFORMS } from './shopPlatforms'

type RangeId = 'today' | 'yesterday' | '7d' | '30d'

const KPIS: { key: 'workbenchKpiGmv' | 'workbenchKpiOrders' | 'workbenchKpiRefund' | 'workbenchKpiViolation' | 'workbenchKpiWithdraw' | 'workbenchKpiGoods' | 'workbenchKpiPending' | 'workbenchKpiDeposit' | 'workbenchKpiInvoice' | 'workbenchKpiTax'; kind: 'money' | 'count' }[] = [
  { key: 'workbenchKpiGmv', kind: 'money' },
  { key: 'workbenchKpiOrders', kind: 'count' },
  { key: 'workbenchKpiRefund', kind: 'money' },
  { key: 'workbenchKpiViolation', kind: 'count' },
  { key: 'workbenchKpiWithdraw', kind: 'money' },
  { key: 'workbenchKpiGoods', kind: 'money' },
  { key: 'workbenchKpiPending', kind: 'money' },
  { key: 'workbenchKpiDeposit', kind: 'money' },
  { key: 'workbenchKpiInvoice', kind: 'count' },
  { key: 'workbenchKpiTax', kind: 'money' },
]

/**
 * 店铺概览：区间筛选 + 经营指标 + 按店明细。
 * 原站依赖客户端读店铺；开源版先把同一套空状态铺上，有绑定数据后再填。
 */
export function ShopOverviewPage() {
  const t = useT()
  const [range, setRange] = useState<RangeId>('today')
  const [notice, setNotice] = useState('')
  const [shops, setShops] = useState<ShopConnection[]>([])
  useEffect(() => {
    if (!isTauriRuntime()) return
    let active = true
    api.shopList().then(items => { if (active) setShops(items) }).catch(() => {})
    return () => { active = false }
  }, [])
  const ranges: { id: RangeId; label: string }[] = [
    { id: 'today', label: t.workbenchRangeToday },
    { id: 'yesterday', label: t.workbenchRangeYesterday },
    { id: '7d', label: t.workbenchRange7d },
    { id: '30d', label: t.workbenchRange30d },
  ]

  return (
    <WorkbenchPage
      className="shop-overview-page"
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavOverview}
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
          <IconButton label={t.workbenchRefresh} size="sm" onClick={() => setNotice(t.workbenchActionSoon)}>
            <RefreshCw size={14} />
          </IconButton>
        </>
      )}
    >
      {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      <p className="workbench-banner">{t.workbenchOverviewClientHint}</p>
      <div className="workbench-kpi-grid">
        {KPIS.map((item) => (
          <div key={item.key} className="workbench-kpi">
            <span className="workbench-kpi-value">{item.kind === 'money' ? '¥0.00' : '0'}</span>
            <span className="workbench-kpi-label">{t[item.key]}</span>
          </div>
        ))}
      </div>

      <WorkbenchCard
        title={t.workbenchOverviewDetail}
        extra={(
          <div className="workbench-tabs">
            <span className="workbench-tab is-active">{t.workbenchFilterAll}<span className="workbench-tab-count">{shops.length}</span></span>
              {SHOP_PLATFORMS.map((item) => (
                <span className="workbench-tab" key={item.id}>{item.name}<span className="workbench-tab-count">{shops.filter(shop => shop.platform === item.id).length}</span></span>
              ))}
          </div>
        )}
      >
        <div className="custom-scrollbar workbench-table-scroll">
          <table className="workbench-table">
            <thead>
              <tr>
                <th>{t.workbenchOverviewColShop}</th>
                <th>{t.workbenchKpiGmv}</th>
                <th>{t.workbenchKpiOrders}</th>
                <th>{t.workbenchKpiRefund}</th>
                <th>{t.workbenchColAction}</th>
              </tr>
            </thead>
          </table>
        </div>
        <WorkbenchEmpty compact icon={<Store size={24} />} title={t.workbenchOverviewEmpty}>
          {t.workbenchOverviewDetailHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
