import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { api } from '../../../api/tauri'
import type { UsageStatsResponse } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { Select } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { aggregateUsage, usageQueryRange, type UsageWindow } from './usageStats'

/**
 * 使用记录：模型调用按天、按模型，加上媒体生成次数。不含积分流水。
 */
export function UsagePage() {
  const t = useT()
  const [days, setDays] = useState<UsageWindow>(7)
  const [stats, setStats] = useState<UsageStatsResponse | null>(null)
  const [tasks, setTasks] = useState<MediaTask[]>([])
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const generation = useRef(0)

  const load = useCallback(async (window: UsageWindow) => {
    const id = ++generation.current
    setLoading(true)
    setError(null)
    try {
      const [nextStats, nextTasks] = await Promise.all([
        api.usageGetStats({ range: usageQueryRange(window), limit: 1 }),
        api.listMediaTasks({}),
      ])
      if (generation.current !== id) return
      setStats(nextStats)
      setTasks(nextTasks)
    } catch (err) {
      if (generation.current !== id) return
      setStats(null)
      setTasks([])
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      if (generation.current === id) setLoading(false)
    }
  }, [])

  useEffect(() => {
    void load(days)
    return () => {
      generation.current += 1
    }
  }, [days, load])

  const view = useMemo(
    () => (stats ? aggregateUsage(stats, tasks, days) : null),
    [stats, tasks, days],
  )

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupStats}
      title={t.workbenchNavUsage}
      subtitle={t.workbenchUsageSubtitle}
      actions={(
        <Select
          ariaLabel={t.workbenchUsageRange}
          value={String(days)}
          onChange={(value) => setDays(Number(value) as UsageWindow)}
          options={[
            { value: '7', label: t.workbenchUsageDays.replace('{n}', '7') },
            { value: '30', label: t.workbenchUsageDays.replace('{n}', '30') },
            { value: '90', label: t.workbenchUsageDays.replace('{n}', '90') },
          ]}
        />
      )}
    >
      {error ? (
        <p className="workbench-inline-note">
          {error}{' '}
          <Button size="sm" onClick={() => void load(days)}>{t.workbenchUsageRetry}</Button>
        </p>
      ) : null}
      {loading && !view ? <p className="workbench-page-sub">{t.workbenchUsageLoading}</p> : null}
      {!error && view?.empty ? <WorkbenchEmpty>{t.workbenchUsageEmpty}</WorkbenchEmpty> : null}
      {!error && view && !view.empty ? (
        <>
          <WorkbenchCard title={t.workbenchUsageByDay}>
            <div className="custom-scrollbar workbench-table-scroll">
              <table className="workbench-table">
                <thead>
                  <tr>
                    <th>{t.workbenchUsageColDay}</th>
                    <th>{t.workbenchUsageColRequests}</th>
                    <th>{t.workbenchUsageColTokens}</th>
                    <th>{t.workbenchUsageColMedia}</th>
                  </tr>
                </thead>
                <tbody>
                  {view.byDay.map((row) => (
                    <tr key={row.date}>
                      <td>{row.label}</td>
                      <td>{row.requests}</td>
                      <td>{row.tokens}</td>
                      <td>{row.media}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </WorkbenchCard>
          <WorkbenchCard title={t.workbenchUsageByModel}>
            <div className="custom-scrollbar workbench-table-scroll">
              <table className="workbench-table">
                <thead>
                  <tr>
                    <th>{t.workbenchUsageColModel}</th>
                    <th>{t.workbenchUsageColRequests}</th>
                    <th>{t.workbenchUsageColTokens}</th>
                    <th>{t.workbenchUsageColMedia}</th>
                  </tr>
                </thead>
                <tbody>
                  {view.byModel.map((row) => (
                    <tr key={row.model}>
                      <td>{row.model}</td>
                      <td>{row.requests}</td>
                      <td>{row.tokens}</td>
                      <td>{row.media}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </WorkbenchCard>
        </>
      ) : null}
    </WorkbenchPage>
  )
}
