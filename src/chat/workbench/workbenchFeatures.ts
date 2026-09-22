import type { I18n } from '../../components/i18n'
import { WORKBENCH_NAV, type WorkbenchPageId } from './workbenchPages'

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
