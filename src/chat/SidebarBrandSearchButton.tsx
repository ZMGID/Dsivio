import { Search } from 'lucide-react'
import { chatTitlebarIconButtonClass } from './platform'

/** 侧栏品牌行右侧的搜索钮：和形态切换器同一行，两种形态共用。 */
export function SidebarBrandSearchButton({
  label,
  active = false,
  onClick,
}: {
  label: string
  active?: boolean
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`${chatTitlebarIconButtonClass} shrink-0 ${
        active ? 'bg-black/[0.05] dark:bg-white/[0.07]' : ''
      }`}
      aria-label={label}
      title={label}
      aria-expanded={active}
    >
      <Search size={16} strokeWidth={1.75} />
    </button>
  )
}
