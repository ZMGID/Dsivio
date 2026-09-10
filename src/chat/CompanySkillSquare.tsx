import { useRef, useState } from 'react'
import { resolveResource } from '@tauri-apps/api/path'
import { BookOpen, Check, Download, Loader2, Search, Sparkles } from 'lucide-react'
import { api, type SkillMeta } from '../api/tauri'
import { Button } from '../components/Button'
import { useT } from '../settings/i18n'
import { ChatMarkdown } from './ChatMarkdown'
import { companySkills, type CompanySkill } from './companySkills'

type Props = {
  skills: SkillMeta[]
  disabledSkillIds: string[]
  loading: boolean
  onLoaded: (skillId: string) => Promise<void>
  items?: readonly CompanySkill[]
}

export function CompanySkillSquare({ skills, disabledSkillIds, loading, onLoaded, items = companySkills }: Props) {
  const t = useT()
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState('')
  const [busyId, setBusyId] = useState<string | null>(null)
  const busy = useRef(false)
  const importedIds = useRef(new Set<string>())
  const [error, setError] = useState('')
  const [expanded, setExpanded] = useState<string | null>(null)
  const categories = Array.from(new Set(items.map((item) => item.category)))
  const normalizedQuery = query.trim().toLowerCase()
  const visible = items.filter((item) => (!category || item.category === category)
    && `${item.name} ${item.description} ${item.category} ${item.author}`.toLowerCase().includes(normalizedQuery))

  async function load(item: CompanySkill) {
    if (busy.current) return
    busy.current = true
    setBusyId(item.id)
    setError('')
    try {
      if (!skills.some((skill) => skill.id === item.id) && !importedIds.current.has(item.id)) {
        if (!/^[a-zA-Z0-9_-]+$/.test(item.directory)) throw new Error(t.chatSkillSquareInvalidPackage)
        const path = await resolveResource(`company-skills/${item.directory}`)
        const result = await api.chatSkillsImport(path)
        if (!result.success || !result.skill) throw new Error(result.error || t.chatSkillInstallFailed)
        if (result.skill.id !== item.id) throw new Error(t.chatSkillSquareIdMismatch)
        // If enabling fails, retry that step without copying the package again.
        importedIds.current.add(item.id)
      }
      await onLoaded(item.id)
    } catch (err) {
      setError(`${item.name}：${err instanceof Error ? err.message : String(err)}`)
    } finally {
      busy.current = false
      setBusyId(null)
    }
  }

  return (
    <section className="space-y-5" aria-label={t.chatSkillTabSquare}>
      <div className="rounded-2xl border border-blue-100 bg-blue-50/60 p-5 dark:border-blue-900/40 dark:bg-blue-950/20">
        <div className="flex items-center gap-2 text-[12px] font-medium text-blue-600 dark:text-blue-400"><Sparkles size={15} />{t.chatSkillSquareBadge}</div>
        <h2 className="mt-2 text-xl font-semibold text-neutral-900 dark:text-neutral-100">{t.chatSkillSquareTitle}</h2>
        <p className="mt-2 max-w-2xl text-[13px] leading-relaxed text-neutral-500 dark:text-neutral-400">{t.chatSkillSquareSubtitle}</p>
      </div>
      <div className="relative">
        <Search size={16} className="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-neutral-400" />
        <input aria-label={t.chatSkillSquareSearch} placeholder={t.chatSkillSquareSearch} value={query} onChange={(event) => setQuery(event.target.value)}
          className="h-10 w-full rounded-md border border-neutral-200 bg-white pl-10 pr-4 text-[14px] outline-none focus:border-blue-400 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100" />
      </div>
      <div className="flex flex-wrap gap-2" aria-label={t.chatSkillSquareCategories}>
        {['', ...categories].map((value) => (
          <button key={value} type="button" aria-pressed={category === value} onClick={() => setCategory(value)}
            className={`rounded-full px-3 py-1.5 text-[12px] transition-colors ${category === value ? 'bg-neutral-900 text-white dark:bg-neutral-100 dark:text-neutral-900' : 'bg-neutral-100 text-neutral-600 dark:bg-neutral-800 dark:text-neutral-400'}`}>
            {value || t.chatSkillSquareAll}
          </button>
        ))}
      </div>
      {error && <p role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-[12px] text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">{error}</p>}
      {items.length === 0 ? (
        <div className="flex min-h-56 flex-col items-center justify-center rounded-xl border border-dashed border-neutral-200 p-6 text-center dark:border-neutral-800">
          <BookOpen size={28} className="text-neutral-400" />
          <h3 className="mt-4 text-[15px] font-medium text-neutral-800 dark:text-neutral-200">{t.chatSkillSquareEmpty}</h3>
          <p className="mt-2 text-[13px] text-neutral-500">{t.chatSkillSquareEmptyHint}</p>
        </div>
      ) : visible.length === 0 ? (
        <p className="py-16 text-center text-[13px] text-neutral-500">{t.chatSkillNoMatchingSkills}</p>
      ) : (
        <div className="grid grid-cols-1 items-start gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {visible.map((item) => {
            const installed = skills.some((skill) => skill.id === item.id)
            const enabled = installed && !disabledSkillIds.includes(item.id)
            return (
              <article key={item.id} className="rounded-xl border border-neutral-200 bg-white p-4 shadow-sm dark:border-neutral-800 dark:bg-neutral-950/40">
                <div className="flex items-center justify-between gap-2">
                  <span className="grid size-10 place-items-center rounded-lg bg-blue-50 text-blue-600 dark:bg-blue-950/40 dark:text-blue-400"><BookOpen size={19} /></span>
                  <span className="text-[11px] text-neutral-400">v{item.version}</span>
                </div>
                <h3 className="mt-3 text-[14px] font-semibold text-neutral-900 dark:text-neutral-100">{item.name}</h3>
                <p className="mt-1 min-h-14 text-[12px] leading-relaxed text-neutral-500 dark:text-neutral-400">{item.description}</p>
                <p className="mt-3 text-[11px] text-neutral-400">{item.category} · {item.author}</p>
                <div className="mt-3 flex items-center justify-between gap-2 border-t border-neutral-100 pt-3 dark:border-neutral-800">
                  <Button size="sm" variant="ghost" aria-expanded={expanded === item.id} aria-controls={`square-detail-${item.id}`} onClick={() => setExpanded(expanded === item.id ? null : item.id)}>{t.chatSkillSquareDetails}</Button>
                  <Button size="sm" disabled={loading || Boolean(busyId) || enabled} onClick={() => void load(item)} aria-label={`${enabled ? t.chatSkillSquareLoaded : t.chatSkillSquareLoad} ${item.name}`}>
                    {busyId === item.id ? <Loader2 size={13} className="animate-spin" /> : enabled ? <Check size={13} /> : <Download size={13} />}
                    {busyId === item.id ? t.chatSkillSquareLoading : enabled ? t.chatSkillSquareLoaded : installed ? t.chatSkillSquareEnable : t.chatSkillSquareLoad}
                  </Button>
                </div>
                {expanded === item.id && <div id={`square-detail-${item.id}`} className="mt-3 border-t border-neutral-100 pt-3 text-[12px] dark:border-neutral-800"><ChatMarkdown content={item.details || item.description} /></div>}
              </article>
            )
          })}
        </div>
      )}
    </section>
  )
}
