import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchHash } from '../workbenchPages'

type ListingTab = 'create' | 'running' | 'waiting' | 'done' | 'failed'

const PLATFORMS = [
  { id: 'douyin', name: '抖店', ready: true },
  { id: 'kuaishou', name: '快手小店', ready: true },
  { id: 'wechat', name: '微信小店', ready: true },
  { id: 'taobao', name: '淘宝', ready: false },
  { id: 'pdd', name: '拼多多', ready: false },
] as const

/**
 * 自动化上架：先看已绑店铺，再按任务状态挑可发布档案。
 * 上架链路还没接，表格保持空。
 */
export function ListingPage() {
  const t = useT()
  const [tab, setTab] = useState<ListingTab>('create')
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
        extra={<span className="workbench-page-sub">{t.workbenchListingManageAll}</span>}
      >
        <div className="workbench-platform-grid">
          {PLATFORMS.map((item) => (
            <div key={item.id} className="workbench-platform-card workbench-platform-card--row">
              <span className="workbench-platform-mark">{item.name.slice(0, 1)}</span>
              <div className="min-w-0">
                <div className="workbench-platform-name">{item.name}</div>
                <div className="workbench-page-sub">
                  {item.ready ? t.workbenchShopsBoundCount.replace('{n}', '0') : t.workbenchComingSoon}
                </div>
              </div>
            </div>
          ))}
          <Button size="sm" onClick={() => { window.location.hash = workbenchHash('shops') }}>{t.workbenchShopsBindTitle}</Button>
        </div>
      </WorkbenchCard>

      <WorkbenchCard>
        <div className="workbench-tabs">
          {tabs.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${tab === item.id ? ' is-active' : ''}`}
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
        <WorkbenchEmpty>{t.workbenchListingEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
