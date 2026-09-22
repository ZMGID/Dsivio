import { useT } from '../../components/i18n'
import { setHash } from '../chatRoutes'
import { listWorkbenchFeatures, type WorkbenchFeature } from './workbenchFeatures'
import { WorkbenchCard } from './WorkbenchPage'
import { WORKBENCH_NAV, workbenchHash, type WorkbenchPageId } from './workbenchPages'

const QUICK_PAGES: readonly WorkbenchPageId[] = ['shops', 'main', 'shorts']

function openPage(page: WorkbenchPageId) {
  setHash(workbenchHash(page))
}

function quickFeatures(features: WorkbenchFeature[]): WorkbenchFeature[] {
  return QUICK_PAGES
    .map((page) => features.find((item) => item.page === page))
    .filter((item): item is WorkbenchFeature => Boolean(item))
}

/** 工作台首页：常用入口 + 按分组列全部功能。点一项就进对应页。 */
export function WorkbenchLanding() {
  const t = useT()
  const features = listWorkbenchFeatures(t)
  const quick = quickFeatures(features)

  return (
    <div className="custom-scrollbar workbench-home">
      <div className="workbench-home-inner">
        <header className="min-w-0">
          <h1 className="workbench-title">{t.productModeWorkbenchName}</h1>
          <p className="workbench-page-sub">{t.workbenchHomeSubtitle}</p>
        </header>

        <WorkbenchCard title={t.workbenchHomeQuick}>
          <div className="workbench-home-quick">
            {quick.map((item) => (
              <button
                key={item.page}
                type="button"
                title={`${item.label} · ${item.group}`}
                className="workbench-home-quick-card"
                onClick={() => openPage(item.page)}
              >
                <span className="workbench-home-quick-name">{item.label}</span>
                <span className="workbench-home-quick-meta">{item.group}</span>
              </button>
            ))}
          </div>
        </WorkbenchCard>

        {WORKBENCH_NAV.groups.map((group) => (
          <WorkbenchCard key={group.id} title={group.label(t)}>
            <div className="workbench-home-links">
              {group.entries.map((entry) => (
                <button
                  key={entry.page}
                  type="button"
                  title={entry.label(t)}
                  className="workbench-home-link"
                  onClick={() => openPage(entry.page)}
                >
                  {entry.label(t)}
                </button>
              ))}
            </div>
          </WorkbenchCard>
        ))}
      </div>
    </div>
  )
}
