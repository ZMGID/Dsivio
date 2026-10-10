export interface NavRowProps {
  icon: React.ReactNode
  label: string
  onClick?: () => void
  disabled?: boolean
  active?: boolean
}

/** 两种形态的侧栏共用的一行导航项：同一套高度、圆角、选中与 hover 语汇。 */
export function NavRow({ icon, label, onClick, disabled, active }: NavRowProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`kv-nav-motion group flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-[13px] text-[color:var(--text)] transition-colors disabled:cursor-default disabled:opacity-40 ${
        active
          ? 'bg-black/[0.06] font-medium dark:bg-white/[0.1]'
          : 'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]'
      }`}
    >
      <span
        className="flex h-5 w-5 shrink-0 items-center justify-center text-[color:var(--text-muted)] transition-colors duration-300 ease-out group-hover:text-[color:var(--text)]"
      >
        {icon}
      </span>
      <span className="min-w-0 flex-1 truncate font-medium">{label}</span>
    </button>
  )
}
