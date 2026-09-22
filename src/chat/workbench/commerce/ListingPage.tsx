import { useEffect, useState } from 'react'
import { api, isTauriRuntime, type ShopConnection } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchHash } from '../workbenchPages'
import { SHOP_PLATFORMS } from './shopPlatforms'
import { ShopPlatformLogo } from './ShopPlatformLogo'

type ListingTab = 'create' | 'running' | 'waiting' | 'done' | 'failed'

/**
 * 自动化上架：先看已绑店铺，再按任务状态挑可发布档案。
 * 上架链路还没接，表格保持空。
 */
export function ListingPage() {
  const t = useT()
  const [tab, setTab] = useState<ListingTab>('create')
  const [shops, setShops] = useState<ShopConnection[]>([])
  useEffect(() => {
    if (!isTauriRuntime()) return
    let active = true
    api.shopList().then(items => { if (active) setShops(items) }).catch(() => {})
    return () => { active = false }
  }, [])
  const tabs: { id: ListingTab; label: string }[] = [
    { id: 'create', label: t.workbenchListingCreate },
    { id: 'running', label: t.workbenchListingRunning },
    { id: 'waiting', label: t.workbenchListingWaiting },
    { id: 'done', label: t.workbenchListingDone },
    { id: 'failed', label: t.workbenchListingFailed },
  ]

  return (
    <WorkbenchPage crumb={t.workbenchGroupCommerce} title={t.workbenchNavListing}>
      <WorkbenchCard
        title={t.workbenchListingPlatforms}
        extra={<Button size="sm" onClick={() => { window.location.hash = workbenchHash('shops') }}>{t.workbenchShopsBindTitle}</Button>}
      >
        <div className="workbench-platform-grid">
          {SHOP_PLATFORMS.map((item) => (
            <div key={item.id} className="workbench-platform-card workbench-platform-card--row">
              <ShopPlatformLogo platform={item.id} />
              <div className="min-w-0">
                <div className="workbench-platform-name">{item.name}</div>
                <div className="workbench-page-sub">
                  {t.workbenchShopsBoundCount.replace('{n}', String(shops.filter(shop => shop.platform === item.id).length))}
                </div>
              </div>
            </div>
          ))}
        </div>
      </WorkbenchCard>

      <WorkbenchCard>
        <div className="custom-scrollbar workbench-tabs workbench-tabs--scroll">
          {tabs.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${tab === item.id ? ' is-active' : ''}`}
              aria-pressed={tab === item.id}
              onClick={() => setTab(item.id)}
            >
              {item.label}
              <span className="workbench-tab-count">0</span>
            </button>
          ))}
        </div>
        <div className="workbench-toolbar">
          <input className="workbench-search" type="search" placeholder={t.workbenchListingSearch} />
          <Button size="sm" variant="primary" disabled>{t.workbenchListingCreateTask}</Button>
        </div>
        <div className="custom-scrollbar workbench-table-scroll">
          <table className="workbench-table">
            <thead>
              <tr>
                <th>{t.workbenchNavProducts}</th>
                <th>{t.workbenchListingColCategory}</th>
                <th>{t.workbenchListingColImage}</th>
                <th>SKU</th>
                <th>{t.workbenchListingColCreated}</th>
                <th>{t.workbenchListingColPlatforms}</th>
                <th>{t.workbenchShopsColStatus}</th>
                <th>{t.workbenchColAction}</th>
              </tr>
            </thead>
          </table>
        </div>
        <WorkbenchEmpty compact>{t.workbenchListingEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
