import { Package } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

/**
 * 选品库：从榜单和同款找货收商品。开源版还没有可收的货，列表保持空。
 */
export function PickLibraryPage() {
  const t = useT()
  return (
    <WorkbenchPage
      crumb={t.workbenchGroupSourcing}
      title={t.workbenchNavPicks}
      subtitle={t.workbenchPicksSubtitle}
      actions={<Button size="sm" disabled>{t.workbenchPicksCreate}</Button>}
    >
      <div className="workbench-page-meta">
        <span>{t.workbenchPicksCount.replace('{n}', '0')}</span>
        <span>{t.workbenchPicksHint}</span>
      </div>
      <WorkbenchCard fill>
        <WorkbenchEmpty icon={<Package size={22} />} title={t.workbenchPicksEmpty}>
          {t.workbenchPicksEmptyHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
