import { memo, useCallback, useEffect, useState } from 'react'
import { ChevronRight, Home } from 'lucide-react'
import { useT } from '../../components/i18n'
import { ChatTitlebarActions } from '../ChatTitlebarActions'
import { ProductModeSwitcher } from '../ProductModeSwitcher'
import { SidebarBrandSearchButton } from '../SidebarBrandSearchButton'
import { SidebarShell } from '../SidebarShell'
import { SidebarUserFooter } from '../SidebarUserFooter'
import { NavRow } from '../SidebarNavRow'
import { chatTitlebarIconButtonClass, chatTitlebarMacInsetClass, usesNativeTitlebar } from '../platform'
import type { SidebarProps } from '../Sidebar'
import { WorkbenchFeatureSearch } from './WorkbenchFeatureSearch'
import { WORKBENCH_NAV, workbenchNavItem, workbenchPageFromHash, type WorkbenchPageId } from './workbenchPages'

const GROUP_STATE_KEY = 'kivio.workbench.navGroups'

function loadGroupOpen(id: string, fallback = true): boolean {
  try {
    const raw = window.localStorage.getItem(GROUP_STATE_KEY)
    if (!raw) return fallback
    const parsed = JSON.parse(raw) as Record<string, boolean>
    return parsed[id] ?? fallback
  } catch {
    return fallback
  }
}

function saveGroupOpen(id: string, open: boolean): void {
  try {
    const raw = window.localStorage.getItem(GROUP_STATE_KEY)
    const parsed = raw ? JSON.parse(raw) as Record<string, boolean> : {}
    parsed[id] = open
    window.localStorage.setItem(GROUP_STATE_KEY, JSON.stringify(parsed))
  } catch {
    /* 写不进去就当这次没记住 */
  }
}

function saveAllGroups(next: Record<string, boolean>): void {
  try {
    window.localStorage.setItem(GROUP_STATE_KEY, JSON.stringify(next))
  } catch {
    /* 写不进去就当这次没记住 */
  }
}

/** 一键展开/收起：两片圆角叠片，收起靠拢、展开留缝。 */
function NavGroupsToggleIcon({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width={16}
      height={16}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {expanded ? (
        <>
          <rect x="5" y="6.5" width="14" height="4.5" rx="1.6" />
          <rect x="5" y="13" width="14" height="4.5" rx="1.6" />
        </>
      ) : (
        <>
          <rect x="5" y="4.6" width="14" height="5" rx="1.7" />
          <rect x="5" y="14.4" width="14" height="5" rx="1.7" />
        </>
      )}
    </svg>
  )
}

export type WorkbenchSidebarProps = Pick<
  SidebarProps,
  | 'lang'
  | 'extensionsActive'
  | 'onOpenExtensionsItem'
  | 'onNewConversation'
  | 'onOpenSettings'
  | 'onSelectLang'
  | 'onOpenUsage'
  | 'settingsActive'
  | 'profileRefreshKey'
  | 'collapsed'
  | 'onToggleCollapsed'
  | 'width'
  | 'onWidthChange'
  | 'productMode'
  | 'onSelectProductMode'
>

/** 工作台侧栏：首页单开，下面按分组列功能。 */
export const WorkbenchSidebar = memo(function WorkbenchSidebar({
  lang,
  onOpenExtensionsItem,
  onNewConversation,
  onOpenSettings,
  onSelectLang,
  onOpenUsage,
  productMode,
  onSelectProductMode,
  settingsActive = false,
  profileRefreshKey = 0,
  collapsed,
  onToggleCollapsed,
  width,
  onWidthChange,
}: WorkbenchSidebarProps) {
  const t = useT()
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>(() => {
    const next: Record<string, boolean> = {}
    for (const group of WORKBENCH_NAV.groups) next[group.id] = loadGroupOpen(group.id)
    return next
  })
  const [activeItem, setActiveItem] = useState(() => workbenchNavItem(workbenchPageFromHash()))
  const [searchOpen, setSearchOpen] = useState(false)

  useEffect(() => {
    const sync = () => setActiveItem(workbenchNavItem(workbenchPageFromHash()))
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  }, [])

  const openPage = useCallback((page: WorkbenchPageId) => {
    onOpenExtensionsItem(workbenchNavItem(page))
    setSearchOpen(false)
  }, [onOpenExtensionsItem])

  const allGroupsOpen = WORKBENCH_NAV.groups.every((group) => openGroups[group.id] ?? true)
  const toggleAllGroups = useCallback(() => {
    const nextOpen = !allGroupsOpen
    const next: Record<string, boolean> = {}
    for (const group of WORKBENCH_NAV.groups) next[group.id] = nextOpen
    setOpenGroups(next)
    saveAllGroups(next)
  }, [allGroupsOpen])

  return (
    <SidebarShell
      collapsed={collapsed}
      settingsActive={settingsActive}
      width={width}
      onWidthChange={onWidthChange}
    >
      {usesNativeTitlebar && (
        <div
          className={`chat-titlebar-row chat-sidebar-titlebar-row flex shrink-0 gap-2 ${chatTitlebarMacInsetClass} pr-3`}
          data-tauri-drag-region
        >
          <ChatTitlebarActions
            sidebarExpanded
            onToggleSidebar={onToggleCollapsed}
            onNewConversation={onNewConversation}
          />
          <div className="min-w-0 flex-1" data-tauri-drag-region />
        </div>
      )}

      <div className="chat-sidebar-brand-row" data-tauri-drag-region="false">
        <div className="min-w-0 flex-1">
          <ProductModeSwitcher mode={productMode} onSelect={onSelectProductMode} />
        </div>
        <button
          type="button"
          onClick={toggleAllGroups}
          className={`${chatTitlebarIconButtonClass} shrink-0`}
          aria-label={allGroupsOpen ? t.workbenchCollapseAllGroups : t.workbenchExpandAllGroups}
          title={allGroupsOpen ? t.workbenchCollapseAllGroups : t.workbenchExpandAllGroups}
          aria-expanded={allGroupsOpen}
        >
          <NavGroupsToggleIcon expanded={allGroupsOpen} />
        </button>
        <SidebarBrandSearchButton
          label={t.workbenchSearchFeatures}
          active={searchOpen}
          onClick={() => setSearchOpen(true)}
        />
      </div>

      <nav
        className="custom-scrollbar flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-2"
        data-tauri-drag-region="false"
      >
        <NavRow
          icon={<Home size={17} />}
          label={WORKBENCH_NAV.home.label(t)}
          active={activeItem === 'workbench'}
          onClick={() => openPage('home')}
        />

        {WORKBENCH_NAV.groups.map((group) => {
          const open = openGroups[group.id] ?? true
          const GroupIcon = group.icon
          return (
            <div key={group.id} className="py-0.5">
              <button
                type="button"
                className="group flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-[13px] font-medium text-neutral-800 transition-colors hover:bg-black/[0.04] dark:text-neutral-200 dark:hover:bg-white/[0.06]"
                aria-expanded={open}
                onClick={() => {
                  const next = !open
                  setOpenGroups((current) => ({ ...current, [group.id]: next }))
                  saveGroupOpen(group.id, next)
                }}
              >
                <span className="flex h-5 w-5 shrink-0 items-center justify-center text-neutral-600 transition duration-300 ease-out group-hover:text-neutral-800 group-active:scale-90 dark:text-neutral-400 dark:group-hover:text-neutral-200">
                  <GroupIcon size={17} />
                </span>
                <span className="min-w-0 flex-1 truncate">{group.label(t)}</span>
                <ChevronRight
                  size={14}
                  strokeWidth={2}
                  className={`shrink-0 text-neutral-400 transition-transform duration-[var(--kv-dur-fast)] ease-[var(--kv-ease-standard)] dark:text-neutral-500 ${
                    open ? 'rotate-90' : ''
                  }`}
                />
              </button>
              {open && (
                <div className="ml-[12px] mt-0.5 grid grid-cols-2 gap-x-1 gap-y-0.5">
                  {group.entries.map((entry) => {
                    const active = activeItem === workbenchNavItem(entry.page)
                    return (
                      <button
                        key={entry.page}
                        type="button"
                        onClick={() => openPage(entry.page)}
                        title={entry.label(t)}
                        className={`workbench-nav-leaf flex min-w-0 items-center px-2 text-left transition-colors ${
                          active
                            ? 'is-active'
                            : 'text-neutral-700 hover:bg-black/[0.04] hover:text-neutral-900 dark:text-neutral-300 dark:hover:bg-white/[0.06] dark:hover:text-neutral-100'
                        }`}
                      >
                        <span className="min-w-0 flex-1 truncate">{entry.label(t)}</span>
                      </button>
                    )
                  })}
                </div>
              )}
            </div>
          )
        })}
      </nav>

      {searchOpen && (
        <WorkbenchFeatureSearch
          activePage={activeItem}
          onSelect={openPage}
          onClose={() => setSearchOpen(false)}
        />
      )}

      <SidebarUserFooter
        lang={lang}
        settingsActive={settingsActive}
        profileRefreshKey={profileRefreshKey}
        onOpenSettings={onOpenSettings}
        onSelectLang={onSelectLang}
        onOpenUsage={onOpenUsage}
      />
    </SidebarShell>
  )
})
