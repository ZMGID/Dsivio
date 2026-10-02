import { useEffect } from 'react'
import { BarChart3 } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchHash } from '../workbenchPages'
import { STAT_COLUMNS, displayedStat } from './publishCatalog'
import { usePublish } from './usePublish'

/**
 * 作品数据：只展示平台真实返回的数字。缺失指标写「平台不支持」，不写 0。
 */
export function PublishDataPage() {
  const t = useT()
  const publish = usePublish()
  const { loadAccounts, loadRecords, loadStats } = publish

  useEffect(() => { void loadAccounts(); void loadRecords() }, [loadAccounts, loadRecords])

  const published = publish.records.filter((record) => record.status === 'published')
  const publishedIds = published.map((record) => record.id).join(',')
  useEffect(() => {
    for (const id of publishedIds.split(',').filter(Boolean)) void loadStats(id)
  }, [publishedIds, loadStats])

  if (publish.accounts.length === 0 && publish.records.length === 0) {
    return (
      <WorkbenchPage crumb={t.workbenchGroupPublish} title={t.workbenchNavPdata} error={publish.error} onErrorDismiss={() => publish.setError('')}>
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

  return (
    <WorkbenchPage crumb={t.workbenchGroupPublish} title={t.workbenchNavPdata} error={publish.error} onErrorDismiss={() => publish.setError('')}>
      <WorkbenchCard title={t.workbenchPdataTitle} hint={t.workbenchPdataHint}>
        {published.length === 0 ? <WorkbenchEmpty compact>{t.workbenchPdataNoPosts}</WorkbenchEmpty> : (
          <div className="custom-scrollbar workbench-table-scroll">
            <table className="workbench-table">
              <thead>
                <tr>
                  <th>{t.workbenchPublishHeadline}</th>
                  {STAT_COLUMNS.map((column) => <th key={column.key}>{t[column.label]}</th>)}
                </tr>
              </thead>
              <tbody>
                {published.map((record) => (
                  <tr key={record.id}>
                    <td>{record.title}</td>
                    {STAT_COLUMNS.map((column) => {
                      const value = displayedStat(publish.statsById[record.id], column.key)
                      return <td key={column.key}>{value === 'pending' ? '—' : value === 'unsupported' ? t.workbenchPublishUnsupported : value}</td>
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
