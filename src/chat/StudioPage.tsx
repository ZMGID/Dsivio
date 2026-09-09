import { type ReactNode } from 'react'
import { ChatTitlebarActions } from './ChatTitlebarActions'
import { chatTitlebarMacInsetClass, chatTitlebarRowClass, usesNativeTitlebar } from './platform'

type StudioPageProps = {
  sidebarCollapsed: boolean
  onToggleSidebar: () => void
  onNewConversation: () => void
  children: ReactNode
}

/**
 * 图片 / 视频工作台外壳。
 *
 * 其它中心页侧栏收起时用绝对定位细带 + pt-12；工作台自己还有 rail/main 的
 * padding-top，两套叠在一起就是那条空白顶栏。收起态改走会话页同一条 in-flow
 * 52px 顶栏（开合 / 新建在栏里），展开态只留 24px 拖拽带、不占行高。
 */
export function StudioPage({
  sidebarCollapsed,
  onToggleSidebar,
  onNewConversation,
  children,
}: StudioPageProps) {
  return (
    <div
      className={`chat-motion-view-in chat-center-page chat-studio-page relative flex min-h-0 min-w-0 flex-1 flex-col${
        usesNativeTitlebar && sidebarCollapsed ? ' chat-studio-page--collapsed' : ''
      }`}
    >
      {usesNativeTitlebar && sidebarCollapsed && (
        <header
          className={`chat-titlebar-row chat-studio-titlebar ${chatTitlebarRowClass} min-w-0 gap-2 ${chatTitlebarMacInsetClass} chat-titlebar-row--collapsed-mac pr-3`}
          data-tauri-drag-region
        >
          <ChatTitlebarActions
            sidebarExpanded={false}
            onToggleSidebar={onToggleSidebar}
            onNewConversation={onNewConversation}
          />
          <div className="min-w-0 flex-1 self-stretch" data-tauri-drag-region />
        </header>
      )}
      {usesNativeTitlebar && !sidebarCollapsed && (
        <div className="absolute inset-x-0 top-0 z-20 h-6" data-tauri-drag-region />
      )}
      {children}
    </div>
  )
}
