import { convertFileSrc } from '@tauri-apps/api/core'
import { ArrowRight, Clock, FileText, Film, Image, Mic, Scissors } from 'lucide-react'
import { Button } from '../../components/Button'
import { useLang, useT, type I18n } from '../../components/i18n'
import type { MediaKind, MediaStatus, MediaTask } from '../../generated/mediaGeneration'
import { setHash } from '../chatRoutes'
import { workbenchFeature, type WorkbenchSubpageId } from './registry'
import { useMediaGeneration } from './useMediaGeneration'
import { recentWorkbenchTasks, taskSourcePage } from './workbenchFeatures'
import { WorkbenchCard, WorkbenchEmpty } from './WorkbenchPage'
import { WORKBENCH_NAV, workbenchHash, type WorkbenchPageId } from './workbenchPages'

const STEPS: readonly { page: WorkbenchSubpageId; label: (t: I18n) => string }[] = [
  { page: 'match', label: (t) => t.workbenchHomeStepSourcing },
  { page: 'main', label: (t) => t.workbenchHomeStepImage },
  { page: 'shorts', label: (t) => t.workbenchHomeStepVideo },
  { page: 'publish', label: (t) => t.workbenchHomeStepPublish },
]

/** 热门工具瓦片：前两张占大格，其余小格；色调只用于区分，不承载语义。 */
const FEATURED: readonly { page: WorkbenchSubpageId; blurb: (t: I18n) => string; tone: 'blue' | 'violet' | 'amber' | 'emerald' | 'rose' | 'slate'; large?: boolean }[] = [
  { page: 'main', blurb: (t) => t.workbenchHomeFeatMain, tone: 'blue', large: true },
  { page: 'shorts', blurb: (t) => t.workbenchHomeFeatShorts, tone: 'violet', large: true },
  { page: 'set-design', blurb: (t) => t.workbenchHomeFeatSetDesign, tone: 'amber' },
  { page: 'match', blurb: (t) => t.workbenchHomeFeatMatch, tone: 'emerald' },
  { page: 'vclone', blurb: (t) => t.workbenchHomeFeatVclone, tone: 'rose' },
  { page: 'listing', blurb: (t) => t.workbenchHomeFeatListing, tone: 'slate' },
]

const RECENT_LIMIT = 6

const KIND_ICONS: Record<MediaKind, typeof Image> = { image: Image, video: Film, speech: Mic, transcribe: FileText, edit: Scissors, text: FileText }

function openPage(page: WorkbenchPageId) {
  setHash(workbenchHash(page))
}

function statusLabel(status: MediaStatus, zh: boolean): string {
  return { running: zh ? '进行中' : 'Running', succeeded: zh ? '完成' : 'Done', failed: zh ? '失败' : 'Failed', cancelled: zh ? '已取消' : 'Cancelled' }[status]
}

function RecentTask({ task }: { task: MediaTask }) {
  const t = useT()
  const zh = useLang() === 'zh'
  const page = taskSourcePage(task.origin)
  if (!page) return null
  const feature = workbenchFeature(page)
  const Icon = KIND_ICONS[task.kind]
  const image = task.outputs.find((output) => output.mime.startsWith('image/'))
  const title = feature.label(t)
  return (
    <button type="button" className="workbench-home-task" title={task.prompt || title} onClick={() => openPage(page)}>
      <span className="workbench-home-task-thumb">
        {image ? <img src={convertFileSrc(image.path)} alt="" loading="lazy" /> : <Icon size={20} />}
      </span>
      <span className="workbench-home-task-body">
        <span className="workbench-home-task-title">{title}</span>
        <span className="workbench-home-task-prompt">{task.prompt || task.error || '—'}</span>
        <span className="workbench-home-task-meta">
          <span className={`workbench-home-task-status is-${task.status}`}>{statusLabel(task.status, zh)}</span>
          <span>{new Date(task.createdAt).toLocaleString()}</span>
        </span>
      </span>
    </button>
  )
}

/** 工作台首页：引导横幅 + 热门工具 + 最近生成 + 全部功能目录。 */
export function WorkbenchLanding() {
  const t = useT()
  const zh = useLang() === 'zh'
  const generation = useMediaGeneration({})
  const recent = recentWorkbenchTasks(generation.tasks, RECENT_LIMIT)

  return (
    <div className="custom-scrollbar workbench-home">
      <div className="workbench-home-inner">
        <section className="workbench-home-hero">
          <div className="workbench-home-hero-text">
            <p className="workbench-home-hero-kicker">{t.productModeWorkbenchName}</p>
            <h1 className="workbench-home-hero-title">{t.workbenchHomeGreeting}</h1>
            <p className="workbench-home-hero-sub">{t.workbenchHomeSubtitle}</p>
          </div>
          <ol className="workbench-home-steps" aria-label={t.workbenchHomeQuick}>
            {STEPS.map((step, index) => {
              const feature = workbenchFeature(step.page)
              const Icon = feature.icon
              return (
                <li key={step.page} className="workbench-home-step">
                  <button type="button" className="workbench-home-step-card" onClick={() => openPage(step.page)}>
                    <span className="workbench-home-step-index">{String(index + 1).padStart(2, '0')}</span>
                    <span className="workbench-home-step-icon"><Icon size={18} /></span>
                    <span className="workbench-home-step-label">{step.label(t)}</span>
                    <span className="workbench-home-step-page">{feature.label(t)}<ArrowRight size={12} /></span>
                  </button>
                </li>
              )
            })}
          </ol>
        </section>

        <section className="workbench-home-section">
          <div className="workbench-home-section-head">
            <h2 className="workbench-home-section-title">{t.workbenchHomeFeatured}</h2>
            <p className="workbench-home-section-hint">{t.workbenchHomeFeaturedHint}</p>
          </div>
          <div className="workbench-home-bento">
            {FEATURED.map((item) => {
              const feature = workbenchFeature(item.page)
              const Icon = feature.icon
              return (
                <button
                  key={item.page}
                  type="button"
                  className={`workbench-home-tile tone-${item.tone}${item.large ? ' is-large' : ''}`}
                  onClick={() => openPage(item.page)}
                >
                  <span className="workbench-home-tile-icon"><Icon size={item.large ? 26 : 20} /></span>
                  <span className="workbench-home-tile-body">
                    <span className="workbench-home-tile-name">{feature.label(t)}</span>
                    <span className="workbench-home-tile-blurb">{item.blurb(t)}</span>
                  </span>
                  <ArrowRight className="workbench-home-tile-arrow" size={16} />
                </button>
              )
            })}
          </div>
        </section>

        <section className="workbench-home-section">
          <div className="workbench-home-section-head">
            <h2 className="workbench-home-section-title">{t.workbenchHomeRecent}</h2>
            <Button size="sm" onClick={() => openPage('assets')}>{t.workbenchHomeRecentAll}<ArrowRight size={14} /></Button>
          </div>
          <WorkbenchCard>
            {recent.length ? (
              <div className="workbench-home-tasks">
                {recent.map((task) => <RecentTask key={task.id} task={task} />)}
              </div>
            ) : (
              <WorkbenchEmpty compact icon={<Clock size={22} />} title={generation.loading ? (zh ? '正在加载…' : 'Loading…') : (zh ? '还没有生成记录' : 'No generations yet')}>
                {generation.error || (zh ? '从上方任一工具开始，结果会出现在这里。' : 'Start from any tool above; results show up here.')}
              </WorkbenchEmpty>
            )}
          </WorkbenchCard>
        </section>

        <section className="workbench-home-section">
          <div className="workbench-home-section-head">
            <h2 className="workbench-home-section-title">{t.workbenchHomeAll}</h2>
          </div>
          <div className="workbench-home-groups">
            {WORKBENCH_NAV.groups.map((group) => {
              const GroupIcon = group.icon
              return (
                <section key={group.id} className="workbench-card-block workbench-home-group">
                  <div className="workbench-home-group-head">
                    <span className="workbench-home-group-icon"><GroupIcon size={18} /></span>
                    <div className="min-w-0">
                      <h3 className="workbench-home-group-title">{group.label(t)}</h3>
                      <p className="workbench-home-group-desc">{group.description(t)}</p>
                    </div>
                    <span className="workbench-home-group-count">{t.workbenchHomeFeatureCount.replace('{n}', String(group.entries.length))}</span>
                  </div>
                  <div className="workbench-home-group-links">
                    {group.entries.map((entry) => {
                      const Icon = entry.icon
                      return (
                        <button key={entry.page} type="button" className="workbench-home-link" title={entry.label(t)} onClick={() => openPage(entry.page)}>
                          {Icon ? <Icon size={14} /> : null}
                          <span>{entry.label(t)}</span>
                        </button>
                      )
                    })}
                  </div>
                </section>
              )
            })}
          </div>
        </section>
      </div>
    </div>
  )
}
