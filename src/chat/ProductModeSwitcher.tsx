import { ArrowRightLeft } from 'lucide-react'
import { useT } from '../components/i18n'
import { chatTitlebarPillButtonClass } from './platform'
import type { ProductMode } from './productMode'

interface ProductModeSwitcherProps {
  mode: ProductMode
  onSelect: (mode: ProductMode) => void
}

/** 形态名集中在这里，触发按钮读这一份。 */
function useProductModeLabels(): Record<ProductMode, string> {
  const t = useT()
  return {
    chat: t.productModeChatName,
    workbench: t.productModeWorkbenchName,
  }
}

export function otherProductMode(mode: ProductMode): ProductMode {
  return mode === 'chat' ? 'workbench' : 'chat'
}

type ModeEnterDirection = 'forward' | 'back'

/** 切到工作台时名字自下而上，切回对话时自上而下。 */
export function productModeEnterDirection(next: ProductMode): ModeEnterDirection {
  return next === 'workbench' ? 'forward' : 'back'
}

/**
 * 侧栏整棵换掉时，切换器会卸载再挂上。方向写在 documentElement 上，
 * 新按钮渲染时读得到；只给名字用，不触发整页过渡。
 */
export function armProductModeEnter(direction: ModeEnterDirection): void {
  const root = document.documentElement
  root.dataset.productModeDir = direction
  window.setTimeout(() => {
    if (root.dataset.productModeDir === direction) delete root.dataset.productModeDir
  }, 240)
}

function readProductModeEnter(): ModeEnterDirection | null {
  const value = document.documentElement.dataset.productModeDir
  return value === 'forward' || value === 'back' ? value : null
}

/**
 * 侧栏左上角的形态切换：点一下换到另一种，再点一下换回来。
 * 没有菜单。落到哪一页由 ChatSidebarPane 决定。
 */
export function ProductModeSwitcher({ mode, onSelect }: ProductModeSwitcherProps) {
  const t = useT()
  const labels = useProductModeLabels()
  const next = otherProductMode(mode)
  const enterDirection = readProductModeEnter()

  return (
    <button
      type="button"
      onClick={() => onSelect(next)}
      className={`${chatTitlebarPillButtonClass} chat-product-mode-trigger max-w-full${
        enterDirection ? ` is-${enterDirection}` : ''
      }`}
      aria-label={t.productModeSwitchTo.replace('{name}', labels[next])}
      title={t.productModeSwitchTo.replace('{name}', labels[next])}
    >
      <span className="chat-product-mode-name min-w-0 truncate font-semibold">{labels[mode]}</span>
      <ArrowRightLeft size={13} strokeWidth={2} className="shrink-0 text-neutral-400 dark:text-neutral-500" />
    </button>
  )
}
