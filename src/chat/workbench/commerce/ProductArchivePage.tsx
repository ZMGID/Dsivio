import { useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

type ArchiveStatus = 'all' | 'incomplete' | 'analyzing' | 'generating' | 'failed' | 'ready'

/**
 * 商品档案：先建档，再按状态看内容生成进度。
 * 还没有档案存储，列表保持空；状态条和工具栏先按原站流程摆好。
 */
export function ProductArchivePage() {
  const t = useT()
  const [status, setStatus] = useState<ArchiveStatus>('all')
  const tabs: { id: ArchiveStatus; label: string }[] = [
    { id: 'all', label: t.workbenchArchiveAll },
    { id: 'incomplete', label: t.workbenchArchiveIncomplete },
    { id: 'analyzing', label: t.workbenchArchiveAnalyzing },
    { id: 'generating', label: t.workbenchArchiveGenerating },
    { id: 'failed', label: t.workbenchArchiveFailed },
    { id: 'ready', label: t.workbenchArchiveReady },
  ]

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavProducts}
      subtitle={t.workbenchArchiveSubtitle}
      actions={(
        <div className="workbench-page-actions">
          <IconButton label={t.workbenchRefresh} size="sm">
            <RefreshCw size={14} />
          </IconButton>
          <Button size="sm" variant="primary">
            <Plus size={14} />
            {t.workbenchArchiveNew}
          </Button>
        </div>
      )}
    >
      <div className="workbench-stat-row">
        {tabs.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`workbench-stat${status === item.id ? ' is-active' : ''}`}
            onClick={() => setStatus(item.id)}
          >
            <span className="workbench-stat-value">0</span>
            <span className="workbench-stat-label">{item.label}</span>
          </button>
        ))}
      </div>

      <WorkbenchCard>
        <div className="workbench-toolbar">
          <input
            className="workbench-search"
            type="search"
            placeholder={t.workbenchArchiveSearch}
          />
        </div>
        <WorkbenchEmpty>{t.workbenchArchiveEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
