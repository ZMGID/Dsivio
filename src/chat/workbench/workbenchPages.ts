import type { ReactNode } from 'react'
import { Bookmark, LayoutDashboard, Package, ScanSearch, ShieldCheck, Store, TrendingUp, Upload, Video, Workflow } from 'lucide-react'
import type { I18n } from '../../components/i18n'
import { hashPath } from '../chatRoutes'
import type { ChatExtensionsNavItem } from '../chatRoutes'

type WorkbenchIcon = (props: { size?: number; className?: string }) => ReactNode

/** 工作台首页之外的页面。路由写在 `#chat/workbench/{id}`，仍属 workbench 这一种 chatView。 */
export type WorkbenchSubpageId =
  | 'shops' | 'overview' | 'products' | 'listing' | 'check' | 'workflows'
  | 'ranks' | 'match' | 'picks'
export type WorkbenchPageId = 'home' | WorkbenchSubpageId

export const WORKBENCH_SUBPAGES: readonly WorkbenchSubpageId[] = [
  'shops',
  'overview',
  'products',
  'listing',
  'check',
  'workflows',
  'ranks',
  'match',
  'picks',
]

export function isWorkbenchSubpage(id: string): id is WorkbenchSubpageId {
  return (WORKBENCH_SUBPAGES as readonly string[]).includes(id)
}

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
  icon?: WorkbenchIcon
}

export interface WorkbenchNavGroup {
  id: string
  label: (t: I18n) => string
  icon: WorkbenchIcon
  entries: readonly WorkbenchNavEntry[]
}

/** 工作台侧栏目录。分组按参考站顺序；ChatGPT 不进开源版。 */
export const WORKBENCH_NAV: {
  home: WorkbenchNavEntry
  groups: readonly WorkbenchNavGroup[]
} = {
  home: { page: 'home', label: (t) => t.workbenchNavHome },
  groups: [
    {
      id: 'commerce',
      label: (t) => t.workbenchGroupCommerce,
      icon: Store,
      entries: [
        { page: 'shops', label: (t) => t.workbenchNavShops, icon: Store },
        { page: 'overview', label: (t) => t.workbenchNavOverview, icon: LayoutDashboard },
        { page: 'products', label: (t) => t.workbenchNavProducts, icon: Package },
        { page: 'listing', label: (t) => t.workbenchNavListing, icon: Upload },
        { page: 'check', label: (t) => t.workbenchNavCheck, icon: ShieldCheck },
        { page: 'workflows', label: (t) => t.workbenchNavWorkflows, icon: Workflow },
      ],
    },
    {
      id: 'sourcing',
      label: (t) => t.workbenchGroupSourcing,
      icon: TrendingUp,
      entries: [
        { page: 'ranks', label: (t) => t.workbenchNavRanks, icon: Video },
        { page: 'match', label: (t) => t.workbenchNavMatch, icon: ScanSearch },
        { page: 'picks', label: (t) => t.workbenchNavPicks, icon: Bookmark },
      ],
    },
  ],
}
