import type { ComponentType, ReactNode } from 'react'
import { BarChart3, Bookmark, Captions, Clapperboard, ClipboardList, Copy, FileText, Film, FolderOpen, History, Image, Images, KeyRound, LayoutDashboard, Package, PanelsTopLeft, Pencil, ScanSearch, Send, Share2, ShieldCheck, Shirt, Shuffle, Sparkles, Store, TrendingUp, Upload, UserRound, Users, Video, Workflow } from 'lucide-react'
import type { I18n } from '../../components/i18n'

/**
 * 工作台功能注册表：这是工作台「有哪些功能」的唯一权威。
 *
 * 侧栏、首页目录、功能搜索、中心区路由和 `chatRoutes` 的导航词表都从这张表派生。
 * 加一个功能 = 建页面文件 + 在 `WORKBENCH_FEATURES` 加一行 + 补 i18n 标签；删一个功能反过来。
 * 不要在别处再枚举功能 id。
 *
 * 这个文件是叶子模块：只允许依赖图标和 i18n 类型，不能 import `chatRoutes` 或页面组件本体。
 */
export type WorkbenchIcon = (props: { size?: number; className?: string }) => ReactNode

export interface WorkbenchGroupDef {
  id: string
  label: (t: I18n) => string
  icon: WorkbenchIcon
}

export interface WorkbenchFeatureDef {
  /** 路由后缀：`#chat/workbench/{id}`。改 id 会让用户已保存的路由失效，视为破坏性变更。 */
  id: string
  group: string
  label: (t: I18n) => string
  icon: WorkbenchIcon
  /** 懒加载页面组件；中心区首次进入该页时才下载它的代码。 */
  load: () => Promise<ComponentType>
}

export const WORKBENCH_GROUPS = [
  { id: 'commerce', label: (t) => t.workbenchGroupCommerce, icon: Store },
  { id: 'sourcing', label: (t) => t.workbenchGroupSourcing, icon: TrendingUp },
  { id: 'copy', label: (t) => t.workbenchGroupCopy, icon: FileText },
  { id: 'image', label: (t) => t.workbenchGroupImage, icon: Image },
  { id: 'video', label: (t) => t.workbenchGroupVideo, icon: Video },
  { id: 'publish', label: (t) => t.workbenchGroupPublish, icon: Share2 },
  { id: 'content', label: (t) => t.workbenchGroupContent, icon: FolderOpen },
  { id: 'stats', label: (t) => t.workbenchGroupStats, icon: History },
] as const satisfies readonly WorkbenchGroupDef[]

export type WorkbenchGroupId = (typeof WORKBENCH_GROUPS)[number]['id']

/** 表内顺序就是侧栏和首页里的展示顺序。 */
export const WORKBENCH_FEATURES = [
  // 电商自动化
  { id: 'shops', group: 'commerce', label: (t) => t.workbenchNavShops, icon: Store, load: () => import('./commerce/ShopBindingPage').then((m) => m.ShopBindingPage) },
  { id: 'overview', group: 'commerce', label: (t) => t.workbenchNavOverview, icon: LayoutDashboard, load: () => import('./commerce/ShopOverviewPage').then((m) => m.ShopOverviewPage) },
  { id: 'products', group: 'commerce', label: (t) => t.workbenchNavProducts, icon: Package, load: () => import('./commerce/ProductArchivePage').then((m) => m.ProductArchivePage) },
  { id: 'listing', group: 'commerce', label: (t) => t.workbenchNavListing, icon: Upload, load: () => import('./commerce/ListingPage').then((m) => m.ListingPage) },
  { id: 'check', group: 'commerce', label: (t) => t.workbenchNavCheck, icon: ShieldCheck, load: () => import('./commerce/ListingCheckPage').then((m) => m.ListingCheckPage) },
  { id: 'workflows', group: 'commerce', label: (t) => t.workbenchNavWorkflows, icon: Workflow, load: () => import('./workflow/WorkflowPage').then((m) => m.WorkflowPage) },
  // 智能选品
  { id: 'ranks', group: 'sourcing', label: (t) => t.workbenchNavRanks, icon: Video, load: () => import('./sourcing/VideoRankPage').then((m) => m.VideoRankPage) },
  { id: 'match', group: 'sourcing', label: (t) => t.workbenchNavMatch, icon: ScanSearch, load: () => import('./sourcing/LookalikePage').then((m) => m.LookalikePage) },
  { id: 'picks', group: 'sourcing', label: (t) => t.workbenchNavPicks, icon: Bookmark, load: () => import('./sourcing/PickLibraryPage').then((m) => m.PickLibraryPage) },
  // 图文创作
  { id: 'posts', group: 'copy', label: (t) => t.workbenchNavPosts, icon: Images, load: () => import('./copy/GraphicPostPage').then((m) => m.GraphicPostPage) },
  { id: 'articles', group: 'copy', label: (t) => t.workbenchNavArticles, icon: FileText, load: () => import('./copy/SeedArticlePage').then((m) => m.SeedArticlePage) },
  // 图片创作
  { id: 'main', group: 'image', label: (t) => t.workbenchNavMain, icon: Image, load: () => import('./image/MainImagePage').then((m) => m.MainImagePage) },
  { id: 'detail', group: 'image', label: (t) => t.workbenchNavDetail, icon: LayoutDashboard, load: () => import('./image/DetailImagePage').then((m) => m.DetailImagePage) },
  { id: 'poster', group: 'image', label: (t) => t.workbenchNavPoster, icon: PanelsTopLeft, load: () => import('./image/PosterPage').then((m) => m.PosterPage) },
  { id: 'retouch', group: 'image', label: (t) => t.workbenchNavRetouch, icon: Sparkles, load: () => import('./image/RetouchPage').then((m) => m.RetouchPage) },
  { id: 'migrate', group: 'image', label: (t) => t.workbenchNavMigrate, icon: Shuffle, load: () => import('./image/MigratePage').then((m) => m.MigratePage) },
  { id: 'dress', group: 'image', label: (t) => t.workbenchNavDress, icon: Shirt, load: () => import('./image/DressPage').then((m) => m.DressPage) },
  { id: 'clone', group: 'image', label: (t) => t.workbenchNavClone, icon: Copy, load: () => import('./image/CloneImagePage').then((m) => m.CloneImagePage) },
  { id: 'edit', group: 'image', label: (t) => t.workbenchNavEdit, icon: Pencil, load: () => import('./image/EditImagePage').then((m) => m.EditImagePage) },
  // 视频创作
  { id: 'shorts', group: 'video', label: (t) => t.workbenchNavShorts, icon: Video, load: () => import('./video/ShortsPage').then((m) => m.ShortsPage) },
  { id: 'avatar', group: 'video', label: (t) => t.workbenchNavAvatar, icon: UserRound, load: () => import('./video/AvatarPage').then((m) => m.AvatarPage) },
  { id: 'drama', group: 'video', label: (t) => t.workbenchNavDrama, icon: Clapperboard, load: () => import('./video/DramaPage').then((m) => m.DramaPage) },
  { id: 'vclone', group: 'video', label: (t) => t.workbenchNavVclone, icon: Copy, load: () => import('./video/VideoClonePage').then((m) => m.VideoClonePage) },
  { id: 'vedit', group: 'video', label: (t) => t.workbenchNavVedit, icon: Film, load: () => import('./video/VideoEditPage').then((m) => m.VideoEditPage) },
  { id: 'subs', group: 'video', label: (t) => t.workbenchNavSubs, icon: Captions, load: () => import('./video/SubtitlePage').then((m) => m.SubtitlePage) },
  // 视频发布
  { id: 'publish', group: 'publish', label: (t) => t.workbenchNavPublish, icon: Send, load: () => import('./publish/PublishPage').then((m) => m.PublishPage) },
  { id: 'vaccts', group: 'publish', label: (t) => t.workbenchNavVaccts, icon: KeyRound, load: () => import('./publish/AccountsPage').then((m) => m.AccountsPage) },
  { id: 'plogs', group: 'publish', label: (t) => t.workbenchNavPlogs, icon: ClipboardList, load: () => import('./publish/PublishLogsPage').then((m) => m.PublishLogsPage) },
  { id: 'pdata', group: 'publish', label: (t) => t.workbenchNavPdata, icon: BarChart3, load: () => import('./publish/PublishDataPage').then((m) => m.PublishDataPage) },
  // 内容管理
  { id: 'roles', group: 'content', label: (t) => t.workbenchNavRoles, icon: Users, load: () => import('./content/RolesPage').then((m) => m.RolesPage) },
  { id: 'assets', group: 'content', label: (t) => t.workbenchNavAssets, icon: Images, load: () => import('./content/AssetLibraryPage').then((m) => m.AssetLibraryPage) },
  // 统计管理
  { id: 'usage', group: 'stats', label: (t) => t.workbenchNavUsage, icon: History, load: () => import('./stats/UsagePage').then((m) => m.UsagePage) },
] as const satisfies readonly (WorkbenchFeatureDef & { group: WorkbenchGroupId })[]

/** 工作台首页之外的页面 id，由注册表推导；路由写在 `#chat/workbench/{id}`。 */
export type WorkbenchSubpageId = (typeof WORKBENCH_FEATURES)[number]['id']
export type WorkbenchPageId = 'home' | WorkbenchSubpageId

const FEATURE_IDS: ReadonlySet<string> = new Set(WORKBENCH_FEATURES.map((feature) => feature.id))

export function isWorkbenchSubpage(id: string): id is WorkbenchSubpageId {
  return FEATURE_IDS.has(id)
}

export function workbenchFeature(id: WorkbenchSubpageId): WorkbenchFeatureDef {
  const feature = WORKBENCH_FEATURES.find((item) => item.id === id)
  if (!feature) throw new Error(`Unknown workbench feature: ${id}`)
  return feature
}

export function workbenchFeaturesInGroup(group: WorkbenchGroupId): readonly WorkbenchFeatureDef[] {
  return WORKBENCH_FEATURES.filter((feature) => feature.group === group)
}
