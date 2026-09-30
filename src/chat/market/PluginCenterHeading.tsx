import type { Lang } from '../../components/i18n'
import './market.css'

type PluginSection = 'market' | 'skill' | 'mcp'

/** 插件、Skill 和 MCP 共用导航；保留各自路由和功能负责人。 */
export function PluginCenterHeading({ value, onChange, lang }: {
  value: PluginSection
  onChange: (value: PluginSection) => void
  lang: Lang
}) {
  const zh = lang === 'zh'
  return (
    <div className="flex flex-wrap items-center gap-3" data-tauri-drag-region="false">
      <h1 className="m-0 text-[28px] font-bold text-[var(--text)]">
        {zh ? '插件市场' : 'Plugin market'}
      </h1>
      <nav className="kv-plugin-segments" aria-label={zh ? '插件类别' : 'Plugin categories'}>
        {([['market', zh ? '插件' : 'Plugins'], ['skill', 'Skill'], ['mcp', 'MCP']] as const).map(([id, label]) => (
          <button key={id} type="button" className="kv-plugin-segment"
            aria-current={value === id ? 'page' : undefined}
            onClick={() => onChange(id)}>
            {label}
          </button>
        ))}
      </nav>
    </div>
  )
}
