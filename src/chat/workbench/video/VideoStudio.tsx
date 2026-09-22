import { useState, type ReactNode } from 'react'
import { MediaTaskList } from '../MediaTaskList'
import type { MediaGeneration } from '../useMediaGeneration'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchCta, WorkbenchPage } from '../WorkbenchPage'
import { VIDEO_TASK_TABS } from './videoCatalog'

export function VideoParamLine({ items }: { items: string[] }) {
  return (
    <div className="workbench-field">
      <div className="workbench-chip-row">
        {items.map((item) => (
          <span key={item} className="workbench-capsule">{item}</span>
        ))}
      </div>
    </div>
  )
}

export function VideoTaskList({ hint, empty, generation }: { hint: string; empty: string; generation?: MediaGeneration }) {
  const t = useT()
  const [tab, setTab] = useState('all')
  const matches = (status: string, filter: string) => filter === 'all' || status === (filter === 'done' ? 'succeeded' : filter)
  const tasks = generation?.tasks || []
  return <WorkbenchCard title={t.workbenchVideoTasks.replace('{n}', String(tasks.length))} hint={hint}>
    <div className="workbench-tabs">{VIDEO_TASK_TABS.filter(item => item.id !== 'queued').map(item => <button key={item.id} type="button" className={`workbench-tab${tab === item.id ? ' is-active' : ''}`} onClick={() => setTab(item.id)}>{t[item.label]}<span className="workbench-tab-count">{tasks.filter(task => matches(task.status, item.id)).length}</span></button>)}</div>
    {generation ? <MediaTaskList bare generation={{ ...generation, tasks: tasks.filter(task => matches(task.status, tab)) }} alt={t.workbenchVideoGenerate} /> : <p className="workbench-page-sub">{empty || t.workbenchVideoTasksEmpty}</p>}
  </WorkbenchCard>
}

/** 左设置 + 右生成（CTA 在右侧）+ 底栏任务列表。 */
export function VideoStudio({
  modelControl,
  generation,
  crumbCurrent,
  title,
  capsules,
  settingsTitle,
  settingsHint,
  settings,
  generateTitle,
  generateHint,
  generate,
  notice,
  cta,
  onGenerate,
  footer,
  taskHint,
  taskEmpty,
}: {
  modelControl?: ReactNode
  generation?: MediaGeneration
  crumbCurrent: string
  title: string
  capsules?: ReactNode
  settingsTitle: string
  settingsHint?: string
  settings: ReactNode
  generateTitle: string
  generateHint?: string
  generate: ReactNode
  notice: string
  cta?: string
  onGenerate?: () => void
  footer?: ReactNode
  taskHint: string
  taskEmpty: string
}) {
  const t = useT()
  return (
    <WorkbenchPage mediaPool={modelControl ? undefined : "videoModels"} fill crumb={t.workbenchGroupVideo} crumbCurrent={crumbCurrent} title={title} actions={capsules}>
      <div className="workbench-video-body">
        <div className="workbench-split workbench-split--even">
          <WorkbenchCard title={settingsTitle} hint={settingsHint}>
            {modelControl}
            <fieldset className="contents" disabled={generation?.busy}>{settings}</fieldset>
          </WorkbenchCard>
          <WorkbenchCard title={generateTitle} hint={generateHint}>
            <fieldset className="contents" disabled={generation?.busy}>{generate}</fieldset>
            <WorkbenchCta notice={notice}>
              {footer ?? (cta && onGenerate ? <Button variant="primary" disabled={generation?.busy} onClick={onGenerate}>{cta}</Button> : null)}
            </WorkbenchCta>
          </WorkbenchCard>
        </div>
        <VideoTaskList hint={taskHint} empty={taskEmpty} generation={generation} />
      </div>
    </WorkbenchPage>
  )
}
