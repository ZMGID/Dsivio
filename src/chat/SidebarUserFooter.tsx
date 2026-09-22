import { useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { api } from '../api/tauri'
import { getUpdateAvailable, subscribeUpdateAvailable, publishUpdateAvailability } from '../api/updateAvailability'
import { CircleArrowUp, Settings } from 'lucide-react'
import { getSettingsCached } from '../api/settingsCache'
import { IconButton } from '../components/Button'
import { i18n, type Lang } from '../components/i18n'
import { isMac } from './platform'
import { SidebarAccountMenu } from './SidebarAccountMenu'
import type { ChatUserProfile } from './types'
import { UserAvatar } from './UserAvatar'

function resolveChatUserProfile(
  chat?: { userDisplayName?: string; userAvatar?: string } | null,
): ChatUserProfile {
  return {
    displayName: chat?.userDisplayName?.trim() || '',
    avatarUrl: chat?.userAvatar?.trim() || '',
  }
}

/**
 * 侧栏底部的账号行：头像、显示名、设置入口与账号菜单。
 * 资料自己从设置里读，调用方只给 `profileRefreshKey` 表示「该重读了」。
 */
export function SidebarUserFooter({
  lang,
  settingsActive,
  profileRefreshKey = 0,
  onOpenSettings,
  onSelectLang,
  onOpenUsage,
}: {
  lang: Lang
  settingsActive: boolean
  profileRefreshKey?: number
  onOpenSettings: () => void
  onSelectLang: (lang: Lang) => void
  onOpenUsage: () => void
}) {
  const [menuRect, setMenuRect] = useState<{ left: number; top: number; width: number } | null>(null)
  const [profile, setProfile] = useState<ChatUserProfile>(() => resolveChatUserProfile())
  const rowRef = useRef<HTMLDivElement>(null)
  const t = i18n[lang]
  const updateAvailable = useSyncExternalStore(subscribeUpdateAvailable, getUpdateAvailable)

  useEffect(() => {
    let cancelled = false
    let unlisten: (() => void) | undefined
    void api.onUpdateAvailable((info) => {
      if (!cancelled) publishUpdateAvailability(info)
    }).then((dispose) => {
      if (cancelled) dispose()
      else unlisten = dispose
    }).catch((error) => console.error('Failed to subscribe to updates:', error))
    return () => { cancelled = true; unlisten?.() }
  }, [])

  useEffect(() => {
    let cancelled = false
    void getSettingsCached().then((settings) => {
      if (!cancelled) setProfile(resolveChatUserProfile(settings.chat))
    }).catch((err) => {
      console.error('Failed to load chat user profile:', err)
    })
    return () => {
      cancelled = true
    }
  }, [profileRefreshKey])

  const toggleMenu = () => {
    if (menuRect) {
      setMenuRect(null)
      return
    }
    const rect = rowRef.current?.getBoundingClientRect()
    if (!rect) return
    setMenuRect({ left: rect.left, top: rect.top, width: rect.width })
  }

  return (
    <div
      className="shrink-0 border-t border-neutral-200/60 p-1.5 dark:border-neutral-800/80"
      data-tauri-drag-region="false"
    >
      <div
        ref={rowRef}
        className={`flex w-full items-center gap-1 rounded-lg px-1.5 py-1 transition-colors ${
          menuRect || settingsActive
            ? 'bg-black/[0.06] dark:bg-white/[0.1]'
            : 'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]'
        }`}
      >
        <button
          type="button"
          onClick={toggleMenu}
          className="flex min-w-0 flex-1 items-center gap-2 text-left"
          aria-haspopup="menu"
          aria-expanded={menuRect !== null}
        >
          <UserAvatar profile={profile} size={22} />
          <span
            className="min-w-0 flex-1 truncate text-[12.5px] text-neutral-700 dark:text-neutral-300"
            title={profile.displayName || undefined}
          >
            {profile.displayName || 'Dsivio'}
          </span>
        </button>
        {updateAvailable && (
          <IconButton
            size="xs"
            label={t.updateAvailable}
            onClick={() => {
              setMenuRect(null)
              onOpenSettings()
            }}
          >
            <CircleArrowUp color="var(--accent)" strokeWidth={1.75} />
          </IconButton>
        )}
        <IconButton
          size="xs"
          label={`${t.settings} (${isMac ? '⌘,' : 'Ctrl+,'})`}
          onClick={() => {
            setMenuRect(null)
            onOpenSettings()
          }}
        >
          <Settings strokeWidth={1.75} />
        </IconButton>
      </div>

      {menuRect && (
        <SidebarAccountMenu
          triggerRect={menuRect}
          lang={lang}
          onSelectLang={onSelectLang}
          onOpenUsage={onOpenUsage}
          onClose={() => setMenuRect(null)}
        />
      )}
    </div>
  )
}
