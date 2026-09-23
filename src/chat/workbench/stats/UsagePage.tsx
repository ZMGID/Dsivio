import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

/**
 * 使用记录：模型调用和操作。开源版不记积分流水。
 */
export function UsagePage() {
  const t = useT()
  return (
    <WorkbenchPage
      crumb={t.workbenchGroupStats}
      title={t.workbenchNavUsage}
      subtitle={t.workbenchUsageSubtitle}
    >
      <WorkbenchCard>
        <div className="custom-scrollbar workbench-table-scroll">
          <table className="workbench-table">
            <thead>
              <tr>
                <th>{t.workbenchPlogsColType}</th>
                <th>{t.workbenchUsageColNote}</th>
                <th>{t.workbenchUsageColTime}</th>
                <th>{t.workbenchShopsColStatus}</th>
              </tr>
            </thead>
          </table>
        </div>
        <WorkbenchEmpty compact>{t.workbenchUsageEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
