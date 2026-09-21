import { memo, useCallback, useState } from 'react'
import { ChevronRight, Home } from 'lucide-react'
import { useT } from '../../components/i18n'
import { ChatTitlebarActions } from '../ChatTitlebarActions'
import { ProductModeSwitcher } from '../ProductModeSwitcher'
import { SidebarShell } from '../SidebarShell'
import { SidebarUserFooter } from '../SidebarUserFooter'
import { NavRow } from '../SidebarNavRow'
import { chatTitlebarMacInsetClass, usesNativeTitlebar } from '../platform'
import type { SidebarProps } from '../Sidebar'
import { WORKBENCH_NAV, workbenchNavItem, type WorkbenchPageId } from './workbenchPages'

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
  extensionsActive = null,
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

  const openPage = useCallback((page: WorkbenchPageId) => {
    onOpenExtensionsItem(workbenchNavItem(page))
  }, [onOpenExtensionsItem])

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
        <ProductModeSwitcher mode={productMode} onSelect={onSelectProductMode} />
      </div>

      <nav
        className="custom-scrollbar flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-2"
        data-tauri-drag-region="false"
      >
        <NavRow
          icon={<Home size={17} />}
          label={WORKBENCH_NAV.home.label(t)}
          active={extensionsActive === 'workbench'}
          onClick={() => openPage('home')}
        />

        {WORKBENCH_NAV.groups.map((group) => {
          const open = openGroups[group.id] ?? true
          const childActive = group.entries.some((entry) => extensionsActive === workbenchNavItem(entry.page))
          const highlighted = open || childActive
          const GroupIcon = group.icon
          return (
            <div key={group.id} className="py-0.5">
              <button
                type="button"
                className={`group flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-[13px] font-medium transition-colors ${
                  highlighted
                    ? 'bg-black/[0.06] text-neutral-900 dark:bg-white/[0.1] dark:text-neutral-50'
                    : 'text-neutral-800 hover:bg-black/[0.04] dark:text-neutral-200 dark:hover:bg-white/[0.06]'
                }`}
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
                    const active = extensionsActive === workbenchNavItem(entry.page)
                    const Icon = entry.icon
                    return (
                      <button
                        key={entry.page}
                        type="button"
                        onClick={() => openPage(entry.page)}
                        className={`workbench-nav-leaf flex items-center gap-2 py-1.5 pl-2 pr-1 text-left transition-colors ${
                          active
                            ? 'is-active'
                            : 'text-neutral-700 hover:bg-black/[0.04] hover:text-neutral-900 dark:text-neutral-300 dark:hover:bg-white/[0.06] dark:hover:text-neutral-100'
                        }`}
                      >
                        <span className={`flex h-4 w-4 shrink-0 items-center justify-center ${
                          active ? '' : 'text-neutral-400 dark:text-neutral-500'
                        }`}
                        >
                          {Icon ? <Icon size={15} /> : null}
                        </span>
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
