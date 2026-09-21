import { useState } from 'react'
import { FolderOpen } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { ASSET_TABS } from './contentCatalog'

/**
 * 图片视频库：本地素材还没落库。不写 30 天过期。
 */
export function AssetLibraryPage() {
  const t = useT()
  const [notice, setNotice] = useState('')

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupContent}
      title={t.workbenchNavAssets}
      subtitle={t.workbenchAssetsSubtitle}
      actions={<Button size="sm" onClick={() => setNotice(t.workbenchAssetsSoon)}>{t.workbenchAssetsUpload}</Button>}
    >
      <WorkbenchCard fill>
        <div className="workbench-tabs">
          {ASSET_TABS.map((item) => (
            <span key={item.id} className={`workbench-tab${item.id === 'all' ? ' is-active' : ''}`}>
              {t[item.label]}
              <span className="workbench-tab-count">0</span>
            </span>
          ))}
        </div>
        {notice ? <p className="workbench-inline-note">{notice}</p> : null}
        <WorkbenchEmpty icon={<FolderOpen size={22} />} title={t.workbenchAssetsEmpty}>
          {t.workbenchAssetsEmptyHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
