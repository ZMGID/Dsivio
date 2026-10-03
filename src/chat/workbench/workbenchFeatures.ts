import type { I18n } from '../../components/i18n'
import type { MediaTask } from '../../generated/mediaGeneration'
import { isWorkbenchSubpage, type WorkbenchSubpageId } from './registry'
import { WORKBENCH_NAV, type WorkbenchPageId } from './workbenchPages'

const ORIGIN_PREFIX = 'workbench/'

/** 任务来源页：`workbench/<page>`；来源不是工作台页面或页面已下线时返回 null。 */
export function taskSourcePage(origin: string | null): WorkbenchSubpageId | null {
  if (!origin?.startsWith(ORIGIN_PREFIX)) return null
  const page = origin.slice(ORIGIN_PREFIX.length)
  return isWorkbenchSubpage(page) ? page : null
}

/** 首页只列工作台页面产生的任务，按创建时间倒序取前几条。 */
export function recentWorkbenchTasks(tasks: readonly MediaTask[], limit: number): MediaTask[] {
  return tasks
    .filter((task) => taskSourcePage(task.origin) !== null)
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    .slice(0, limit)
}

/** 首页目录与功能搜索共用的扁平视图，从 WORKBENCH_NAV 派生。 */
export interface WorkbenchFeature {
  page: WorkbenchPageId
  label: string
  group: string
}

export function listWorkbenchFeatures(t: I18n): WorkbenchFeature[] {
  const home: WorkbenchFeature = {
    page: WORKBENCH_NAV.home.page,
    label: WORKBENCH_NAV.home.label(t),
    group: '',
  }
  const rest = WORKBENCH_NAV.groups.flatMap((group) => {
    const groupLabel = group.label(t)
    return group.entries.map((entry) => ({
      page: entry.page,
      label: entry.label(t),
      group: groupLabel,
    }))
  })
  return [home, ...rest]
}

export function filterWorkbenchFeatures(features: WorkbenchFeature[], query: string): WorkbenchFeature[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return features
  return features.filter((item) => (
    item.label.toLowerCase().includes(needle)
    || item.group.toLowerCase().includes(needle)
  ))
}
