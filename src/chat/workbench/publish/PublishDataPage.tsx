import { BarChart3 } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchHash } from '../workbenchPages'

/**
 * 数据分析：看已授权账号的作品。没有账号时保持空。
 */
export function PublishDataPage() {
  const t = useT()

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupPublish}
      title={t.workbenchNavPdata}
      actions={<span className="workbench-capsule">{t.workbenchRange30d}</span>}
    >
      <WorkbenchCard fill title={t.workbenchPdataTitle} hint={t.workbenchPdataHint}>
        <WorkbenchEmpty icon={<BarChart3 size={22} />} title={t.workbenchPdataEmpty}>
          {t.workbenchPdataEmptyHint}
        </WorkbenchEmpty>
        <div className="workbench-cta-row">
          <Button size="sm" onClick={() => { window.location.hash = workbenchHash('vaccts') }}>
            {t.workbenchPdataGoAccounts}
          </Button>
        </div>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
