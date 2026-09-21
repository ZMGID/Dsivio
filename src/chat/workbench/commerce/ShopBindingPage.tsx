import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

type ShopPlatform = {
  id: string
  name: string
  ready: boolean
}

const PLATFORMS: readonly ShopPlatform[] = [
  { id: 'douyin', name: '抖店', ready: true },
  { id: 'kuaishou', name: '快手小店', ready: true },
  { id: 'wechat', name: '微信小店', ready: true },
  { id: 'taobao', name: '淘宝', ready: false },
  { id: 'pdd', name: '拼多多', ready: false },
]

/**
 * 店铺绑定：选平台授权，下面是已绑定列表。
 * 开源版还没接各平台 OAuth，立即绑定只说明现状，不假装已经绑上。
 */
export function ShopBindingPage() {
  const t = useT()
  const [platform, setPlatform] = useState('douyin')
  const [notice, setNotice] = useState<string | null>(null)

  return (
    <WorkbenchPage crumb={t.workbenchGroupCommerce} title={t.workbenchNavShops} subtitle={t.workbenchShopsSubtitle}>
      <WorkbenchCard title={t.workbenchShopsBindTitle}>
        <p className="workbench-page-sub workbench-page-sub--flush">{t.workbenchShopsBindHint}</p>
        <div className="workbench-platform-grid">
          {PLATFORMS.map((item) => (
            <div key={item.id} className="workbench-platform-card">
              <span className="workbench-platform-mark">{item.name.slice(0, 1)}</span>
              <span className="workbench-platform-name">{item.name}</span>
              <Button
                size="sm"
                variant={item.ready ? 'primary' : 'default'}
                disabled={!item.ready}
                onClick={() => setNotice(t.workbenchShopsBindSoon)}
              >
                {item.ready ? t.workbenchShopsBindNow : t.workbenchComingSoon}
              </Button>
            </div>
          ))}
        </div>
        {notice ? <p className="workbench-inline-note">{notice}</p> : null}
      </WorkbenchCard>

      <WorkbenchCard
        title={t.workbenchShopsBoundTitle}
        extra={<span className="workbench-page-sub">{t.workbenchShopsBoundCount.replace('{n}', '0')}</span>}
      >
        <div className="workbench-tabs">
          {PLATFORMS.filter((item) => item.ready).map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${platform === item.id ? ' is-active' : ''}`}
              onClick={() => setPlatform(item.id)}
            >
              {item.name}
              <span className="workbench-tab-count">0</span>
            </button>
          ))}
        </div>
        <table className="workbench-table">
          <thead>
            <tr>
              <th>{t.workbenchShopsColName}</th>
              <th>{t.workbenchShopsColId}</th>
              <th>{t.workbenchShopsColBoundAt}</th>
              <th>{t.workbenchShopsColStatus}</th>
              <th>{t.workbenchColAction}</th>
            </tr>
          </thead>
        </table>
        <WorkbenchEmpty>{t.workbenchShopsEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
