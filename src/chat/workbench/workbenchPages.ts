import type { I18n } from '../../components/i18n'
import { hashPath } from '../chatRoutes'
import type { ChatExtensionsNavItem } from '../chatRoutes'
import {
  WORKBENCH_FEATURES,
  WORKBENCH_GROUPS,
  isWorkbenchSubpage,
  type WorkbenchFeatureDef,
  type WorkbenchGroupDef,
  type WorkbenchPageId,
} from './registry'

export { isWorkbenchSubpage, type WorkbenchPageId, type WorkbenchSubpageId } from './registry'

/**
 * 工作台路由词表：hash ↔ 页面 id ↔ 侧栏导航项。
 * 有哪些页面看 `registry.ts`；这里只负责地址格式。
 */
export function workbenchHash(page: WorkbenchPageId): string {
  return page === 'home' ? '#chat/workbench' : `#chat/workbench/${page}`
}

export function workbenchPageFromPath(path: string): WorkbenchPageId {
  if (path === 'chat/workbench') return 'home'
  if (path.startsWith('chat/workbench/')) {
    const id = path.slice('chat/workbench/'.length).split('/')[0]
    if (isWorkbenchSubpage(id)) return id
  }
  return 'home'
}

export function workbenchPageFromHash(): WorkbenchPageId {
  return workbenchPageFromPath(hashPath())
}

export function workbenchWorkflowHash(id: string): string {
  return `#chat/workbench/workflows/${encodeURIComponent(id)}`
}

export function workbenchWorkflowIdFromPath(path: string): string | null {
  const prefix = 'chat/workbench/workflows/'
  if (!path.startsWith(prefix)) return null
  const rest = path.slice(prefix.length)
  if (!rest || rest.includes('/')) return null
  try {
    return decodeURIComponent(rest)
  } catch {
    return null
  }
}

export function workbenchWorkflowIdFromHash(): string | null {
  return workbenchWorkflowIdFromPath(hashPath())
}

export function workbenchNavItem(page: WorkbenchPageId): ChatExtensionsNavItem {
  return page === 'home' ? 'workbench' : `workbench/${page}`
}

export interface WorkbenchNavEntry {
  page: WorkbenchPageId
  label: (t: I18n) => string
  icon?: WorkbenchFeatureDef['icon']
}

export interface WorkbenchNavGroup {
  id: string
  label: (t: I18n) => string
  icon: WorkbenchGroupDef['icon']
  entries: readonly WorkbenchNavEntry[]
}

/** 侧栏 / 首页 / 搜索共用的目录视图，按注册表分组顺序派生；空分组不显示。 */
export const WORKBENCH_NAV: {
  home: WorkbenchNavEntry
  groups: readonly WorkbenchNavGroup[]
} = {
  home: { page: 'home', label: (t) => t.workbenchNavHome },
  groups: WORKBENCH_GROUPS.map((group) => ({
    id: group.id,
    label: group.label,
    icon: group.icon,
    entries: WORKBENCH_FEATURES
      .filter((feature) => feature.group === group.id)
      .map((feature) => ({ page: feature.id, label: feature.label, icon: feature.icon })),
  })).filter((group) => group.entries.length > 0),
}
