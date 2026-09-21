import { useCallback, useLayoutEffect, useRef, type PointerEvent as ReactPointerEvent, type ReactNode } from 'react'
import { clampSidebarWidth, SIDEBAR_DEFAULT_WIDTH } from './persistence'

function applySidebarWidthCss(aside: HTMLElement | null, nextWidth: number) {
  const px = `${nextWidth}px`
  aside?.style.setProperty('--chat-sidebar-width', px)
  const shell = aside?.closest('.chat-window-shell')
  if (shell instanceof HTMLElement) {
    shell.style.setProperty('--chat-sidebar-width', px)
  }
}

function setSidebarResizing(aside: HTMLElement | null, resizing: boolean) {
  const shell = aside?.closest('.chat-window-shell')
  if (shell instanceof HTMLElement) {
    shell.classList.toggle('is-sidebar-resizing', resizing)
  }
}

interface SidebarShellProps {
  collapsed: boolean
  settingsActive?: boolean
  /** 展开态宽度。拖拽过程只写 CSS 变量，松手才回传。 */
  width?: number
  onWidthChange?: (width: number) => void
  children: ReactNode
}

/**
 * 左侧栏的外壳：宽度变量、折叠语义、拖拽改宽。
 *
 * 两种形态（对话 / 工作台）各自只管栏里放什么；宽度与折叠是同一份窗口布局规则，
 * 由这里独占，避免两根侧栏各留一套拖拽实现后再各自漂移。
 */
export function SidebarShell({
  collapsed,
  settingsActive = false,
  width = SIDEBAR_DEFAULT_WIDTH,
  onWidthChange,
  children,
}: SidebarShellProps) {
  const asideRef = useRef<HTMLElement>(null)
  const dragStateRef = useRef<{ startX: number; startWidth: number; width: number; raf: number } | null>(null)

  // 折叠后侧栏仍挂载（用于滑出动画），用 inert 让其退出 tab 序 / 不可点击 / 不进 a11y 树。
  // useLayoutEffect：在绘制前与 JSX 里的 aria-hidden 原子地一起生效，避免短暂可聚焦窗口。
  useLayoutEffect(() => {
    const el = asideRef.current
    if (el) el.inert = collapsed
  }, [collapsed])

  useLayoutEffect(() => {
    applySidebarWidthCss(asideRef.current, width)
  }, [width])

  const handleResizeStart = useCallback(
    (event: ReactPointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return
      event.preventDefault()
      event.stopPropagation()
      const aside = asideRef.current
      const measured = aside?.getBoundingClientRect().width ?? 0
      const startWidth = measured > 0 ? measured : width
      dragStateRef.current = { startX: event.clientX, startWidth, width: startWidth, raf: 0 }
      setSidebarResizing(aside, true)

      const onMove = (moveEvent: PointerEvent) => {
        const state = dragStateRef.current
        if (!state) return
        const nextWidth = clampSidebarWidth(state.startWidth + (moveEvent.clientX - state.startX), window.innerWidth)
        state.width = nextWidth
        if (!state.raf) {
          state.raf = window.requestAnimationFrame(() => {
            state.raf = 0
            applySidebarWidthCss(asideRef.current, state.width)
          })
        }
      }
      const onUp = () => {
        window.removeEventListener('pointermove', onMove)
        window.removeEventListener('pointerup', onUp)
        const state = dragStateRef.current
        dragStateRef.current = null
        setSidebarResizing(asideRef.current, false)
        if (!state) return
        if (state.raf) window.cancelAnimationFrame(state.raf)
        applySidebarWidthCss(asideRef.current, state.width)
        if (state.width !== Math.round(startWidth)) onWidthChange?.(state.width)
      }
      window.addEventListener('pointermove', onMove)
      window.addEventListener('pointerup', onUp)
    },
    [onWidthChange, width],
  )

  return (
    <aside
      ref={asideRef}
      className={`chat-sidebar-shell relative flex shrink-0 flex-col overflow-hidden${
        collapsed ? ' is-collapsed' : ''
      }${settingsActive ? ' is-settings-cover' : ''}`}
      aria-hidden={collapsed}
    >
      {!collapsed && (
        <div
          className="chat-sidebar-resize"
          data-tauri-drag-region="false"
          onPointerDown={handleResizeStart}
        />
      )}
      {children}
    </aside>
  )
}
