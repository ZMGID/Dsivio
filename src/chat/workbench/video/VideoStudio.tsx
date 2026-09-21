import type { ReactNode } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchPage } from '../WorkbenchPage'
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

export function VideoTaskList({ hint, empty }: { hint: string; empty: string }) {
  const t = useT()
  return (
    <WorkbenchCard title={t.workbenchVideoTasks.replace('{n}', '0')} hint={hint}>
      <div className="workbench-tabs">
        {VIDEO_TASK_TABS.map((item) => (
          <span key={item.id} className={`workbench-tab${item.id === 'all' ? ' is-active' : ''}`}>
            {t[item.label]}
            <span className="workbench-tab-count">0</span>
          </span>
        ))}
      </div>
      <p className="workbench-page-sub">{empty || t.workbenchVideoTasksEmpty}</p>
    </WorkbenchCard>
  )
}

/** 左设置 + 右生成（CTA 在右侧）+ 底栏任务列表。 */
export function VideoStudio({
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
    <WorkbenchPage fill crumb={t.workbenchGroupVideo} crumbCurrent={crumbCurrent} title={title} actions={capsules}>
      <div className="workbench-video-body">
        <div className="workbench-split workbench-split--even">
          <WorkbenchCard title={settingsTitle} hint={settingsHint}>
            <div className="workbench-card-scroll custom-scrollbar">{settings}</div>
          </WorkbenchCard>
          <WorkbenchCard title={generateTitle} hint={generateHint}>
            <div className="workbench-card-scroll custom-scrollbar">
              {generate}
              {notice ? <p className="workbench-inline-note">{notice}</p> : null}
            </div>
            <div className="workbench-cta-row">
              {footer ?? (cta && onGenerate ? <Button variant="primary" onClick={onGenerate}>{cta}</Button> : null)}
            </div>
          </WorkbenchCard>
        </div>
        <VideoTaskList hint={taskHint} empty={taskEmpty} />
      </div>
    </WorkbenchPage>
  )
}
