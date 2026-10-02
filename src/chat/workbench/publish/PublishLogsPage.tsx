import { useEffect, useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { PUBLISH_LOG_TABS, QUEUED_STATUSES, publishStatusLabel } from './publishCatalog'
import { usePublish } from './usePublish'

/**
 * 发布记录：全部 / 待发布（上传中、处理中、结果不确定）/ 已发布。失败和拒绝只出现在全部。
 */
export function PublishLogsPage() {
  const t = useT()
  const publish = usePublish()
  const { loadRecords } = publish
  const [tab, setTab] = useState<(typeof PUBLISH_LOG_TABS)[number]['id']>('all')

  useEffect(() => { void loadRecords() }, [loadRecords])

  const visible = publish.records.filter((record) => {
    if (tab === 'queued') return QUEUED_STATUSES.includes(record.status)
    if (tab === 'done') return record.status === 'published'
    return true
  })
  const count = (id: (typeof PUBLISH_LOG_TABS)[number]['id']) => {
    if (id === 'queued') return publish.records.filter((record) => QUEUED_STATUSES.includes(record.status)).length
    if (id === 'done') return publish.records.filter((record) => record.status === 'published').length
    return publish.records.length
  }

  return (
    <WorkbenchPage crumb={t.workbenchGroupPublish} title={t.workbenchNavPlogs} error={publish.error} onErrorDismiss={() => publish.setError('')}>
      <WorkbenchCard>
        <div className="workbench-tabs">
          {PUBLISH_LOG_TABS.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`workbench-tab${tab === item.id ? ' is-active' : ''}`}
              onClick={() => setTab(item.id)}
            >
              {t[item.label]}
              <span className="workbench-tab-count">{count(item.id)}</span>
            </button>
          ))}
        </div>
        <div className="custom-scrollbar workbench-table-scroll">
          <table className="workbench-table">
            <thead>
              <tr>
                <th>{t.workbenchPublishHeadline}</th>
                <th>{t.workbenchVacctsColPlatform}</th>
                <th>{t.workbenchShopsColStatus}</th>
                <th>{t.workbenchPlogsColTime}</th>
                <th>{t.workbenchColAction}</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((record) => (
                <tr key={record.id}>
                  <td>{record.title}</td>
                  <td>{record.platform === 'tiktok' ? t.workbenchVacctsPlatformTiktok : t.workbenchVacctsPlatformYoutube}</td>
                  <td>{t[publishStatusLabel(record.status)]}{record.reason ? ` · ${record.reason}` : ''}</td>
                  <td>{record.updatedAt}</td>
                  <td>
                    <Button size="sm" disabled={publish.pending} onClick={() => { void publish.refreshRecord(record.id) }}>{t.workbenchPlogsRefresh}</Button>
                    {record.status === 'failed' || record.status === 'rejected' ? (
                      <Button size="sm" disabled={publish.pending} onClick={() => { void publish.retry(record.id) }}>{t.workbenchPlogsRetry}</Button>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {visible.length === 0 ? <WorkbenchEmpty compact>{t.workbenchPlogsEmpty}</WorkbenchEmpty> : null}
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
