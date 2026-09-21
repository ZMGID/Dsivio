import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Check, ChevronDown } from 'lucide-react'
import { useT } from '../components/i18n'
import { chatTitlebarPillButtonClass } from './platform'
import { useClampedMenuPosition } from './useClampedMenuPosition'
import { useCloseAnimation } from './useCloseAnimation'
import { PRODUCT_MODES, type ProductMode } from './productMode'

interface ProductModeSwitcherProps {
  mode: ProductMode
  onSelect: (mode: ProductMode) => void
}

/** 形态名集中在这里，触发按钮和下拉项读同一份。 */
function useProductModeLabels(): Record<ProductMode, string> {
  const t = useT()
  return {
    chat: t.productModeChatName,
    workbench: t.productModeWorkbenchName,
  }
}

function ProductModeMenu({
  anchor,
  mode,
  onSelect,
  onClose: onCloseProp,
  triggerRef,
}: {
  anchor: { left: number; top: number }
  mode: ProductMode
  onSelect: (mode: ProductMode) => void
  onClose: () => void
  triggerRef: React.RefObject<HTMLElement | null>
}) {
  const labels = useProductModeLabels()
  const menuRef = useRef<HTMLDivElement>(null)
  const { closing, startClose, onAnimationEnd } = useCloseAnimation(onCloseProp)
  const position = useClampedMenuPosition(menuRef, anchor)

  useEffect(() => {
    const onPointerDown = (e: MouseEvent) => {
      const target = e.target as Node
      if (menuRef.current?.contains(target)) return
      // 触发按钮自己 toggle，别让外部关闭抢先关掉再被点开。
      if (triggerRef.current?.contains(target)) return
      startClose()
    }
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') startClose()
    }
    window.addEventListener('mousedown', onPointerDown)
    window.addEventListener('keydown', onKeyDown)
    return () => {
      window.removeEventListener('mousedown', onPointerDown)
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [startClose, triggerRef])

  const menu = (
    <div
      ref={menuRef}
      className={`kv-menu ${closing ? 'chat-motion-popover-out' : 'chat-motion-popover chat-motion-menu-cascade'} fixed z-[200] min-w-[160px]`}
      style={{ left: position.left, top: position.top }}
      role="menu"
      onAnimationEnd={onAnimationEnd}
    >
      {PRODUCT_MODES.map((candidate) => {
        const active = candidate === mode
        return (
          <button
            key={candidate}
            type="button"
            role="menuitemradio"
            aria-checked={active}
            className="kv-menu-item"
            onClick={() => {
              onSelect(candidate)
              startClose()
            }}
          >
            <span className="min-w-0 flex-1 truncate">{labels[candidate]}</span>
            {active && <Check size={15} strokeWidth={2} className="shrink-0 opacity-70" />}
          </button>
        )
      })}
    </div>
  )

  return createPortal(menu, document.body)
}

/**
 * 侧栏左上角的形态切换器：在「对话」与「工作台」之间切换整根左侧导航。
 * 只负责表达意图，落到哪一页由 Chat.tsx 的形态负责人决定（见 productModeHomeHash）。
 */
export function ProductModeSwitcher({ mode, onSelect }: ProductModeSwitcherProps) {
  const t = useT()
  const labels = useProductModeLabels()
  const triggerRef = useRef<HTMLButtonElement>(null)
  const [anchor, setAnchor] = useState<{ left: number; top: number } | null>(null)

  const toggle = () => {
    if (anchor) {
      setAnchor(null)
      return
    }
    const rect = triggerRef.current?.getBoundingClientRect()
    if (!rect) return
    setAnchor({ left: rect.left, top: rect.bottom + 6 })
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={toggle}
        className={`${chatTitlebarPillButtonClass} chat-product-mode-trigger max-w-full`}
        aria-haspopup="menu"
        aria-expanded={anchor !== null}
        aria-label={t.productModeSwitch}
        title={t.productModeSwitch}
      >
        <span className="min-w-0 truncate font-semibold">{labels[mode]}</span>
        <ChevronDown
          size={14}
          strokeWidth={2}
          className={`shrink-0 text-neutral-400 transition-transform duration-[var(--kv-dur-fast)] ease-[var(--kv-ease-standard)] dark:text-neutral-500 ${
            anchor ? 'rotate-180' : ''
          }`}
        />
      </button>
      {anchor && (
        <ProductModeMenu
          anchor={anchor}
          mode={mode}
          onSelect={onSelect}
          onClose={() => setAnchor(null)}
          triggerRef={triggerRef}
        />
      )}
    </>
  )
}
