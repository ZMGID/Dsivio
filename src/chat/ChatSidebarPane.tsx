import { memo, useCallback, useState, Profiler, type ProfilerOnRenderCallback } from 'react'
import { Sidebar, type SidebarProps } from './Sidebar'
import { WorkbenchSidebar } from './workbench/WorkbenchSidebar'
import { useConversationTransition } from './conversationTransitionStore'
import { loadProductMode, saveProductMode, type ProductMode } from './productMode'

export interface ChatSidebarPaneProps extends Omit<SidebarProps, 'productMode' | 'onSelectProductMode'> {
  onRender: ProfilerOnRenderCallback
}

/**
 * 侧栏的 React 子树边界。
 *
 * Chat 仍负责协调路由和会话，但侧栏自己的状态/数据更新不应该把聊天主区
 * 一起带进协调。memo 让侧栏只在真正的侧栏输入变化时重渲染，Profiler 也
 * 留在这个边界内，便于分别观察侧栏和主区的成本。
 *
 * 形态（对话 / 工作台）只决定这一列长什么样，所以状态就留在这个边界里，
 * 不往 Chat 抬 —— 中心区看的是路由里的 chatView，跟形态无关。
 */
export const ChatSidebarPane = memo(function ChatSidebarPane({ onRender, ...props }: ChatSidebarPaneProps) {
  const transition = useConversationTransition()
  const { onNewConversation, onOpenExtensionsItem } = props
  const [productMode, setProductMode] = useState<ProductMode>(loadProductMode)

  /**
   * 切形态后落到该形态的首页。两条去处都用侧栏本来就有的导航回调，
   * 它们已经处理过设置页退场，这里不需要再包一层。
   */
  const handleSelectProductMode = useCallback((next: ProductMode) => {
    setProductMode(next)
    saveProductMode(next)
    if (next === 'workbench') onOpenExtensionsItem('workbench')
    else onNewConversation()
  }, [onNewConversation, onOpenExtensionsItem])

  return (
    <Profiler id="Sidebar" onRender={onRender}>
      {productMode === 'workbench' ? (
        <WorkbenchSidebar
          {...props}
          productMode={productMode}
          onSelectProductMode={handleSelectProductMode}
        />
      ) : (
        <Sidebar
          {...props}
          currentConversationId={transition.targetConversationId ?? props.currentConversationId}
          productMode={productMode}
          onSelectProductMode={handleSelectProductMode}
        />
      )}
    </Profiler>
  )
})
