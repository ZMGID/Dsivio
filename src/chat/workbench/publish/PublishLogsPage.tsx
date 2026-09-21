import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { PUBLISH_LOG_TABS } from './publishCatalog'

/**
 * 发布记录：历史任务表。还没有任务。
 */
export function PublishLogsPage() {
  const t = useT()
  return (
    <WorkbenchPage crumb={t.workbenchGroupPublish} title={t.workbenchNavPlogs}>
      <WorkbenchCard>
        <div className="workbench-tabs">
          {PUBLISH_LOG_TABS.map((item) => (
            <span key={item.id} className={`workbench-tab${item.id === 'all' ? ' is-active' : ''}`}>
              {t[item.label]}
              <span className="workbench-tab-count">0</span>
            </span>
          ))}
        </div>
        <table className="workbench-table">
          <thead>
            <tr>
              <th>{t.workbenchPlogsColCover}</th>
              <th>{t.workbenchPublishHeadline}</th>
              <th>{t.workbenchPlogsColType}</th>
              <th>{t.workbenchVacctsColPlatform}</th>
              <th>{t.workbenchShopsColStatus}</th>
              <th>{t.workbenchPlogsColTime}</th>
              <th>{t.workbenchColAction}</th>
            </tr>
          </thead>
        </table>
        <WorkbenchEmpty>{t.workbenchPlogsEmpty}</WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
