import { useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { Search } from 'lucide-react'
import { useT, type I18n } from '../../components/i18n'
import { HighlightText } from '../searchHighlight'
import { WORKBENCH_NAV, workbenchNavItem, type WorkbenchPageId } from './workbenchPages'

export interface WorkbenchFeature {
  page: WorkbenchPageId
  label: string
  group: string
}

export function listWorkbenchFeatures(t: I18n): WorkbenchFeature[] {
  const home: WorkbenchFeature = {
    page: WORKBENCH_NAV.home.page,
    label: WORKBENCH_NAV.home.label(t),
    group: '',
  }
  const rest = WORKBENCH_NAV.groups.flatMap((group) => {
    const groupLabel = group.label(t)
    return group.entries.map((entry) => ({
      page: entry.page,
      label: entry.label(t),
      group: groupLabel,
    }))
  })
  return [home, ...rest]
}

export function filterWorkbenchFeatures(features: WorkbenchFeature[], query: string): WorkbenchFeature[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return features
  return features.filter((item) => (
    item.label.toLowerCase().includes(needle)
    || item.group.toLowerCase().includes(needle)
  ))
}

export function WorkbenchFeatureSearch({
  activePage,
  onSelect,
  onClose,
}: {
  activePage: string
  onSelect: (page: WorkbenchPageId) => void
  onClose: () => void
}) {
  const t = useT()
  const inputRef = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const features = useMemo(() => listWorkbenchFeatures(t), [t])
  const results = useMemo(() => filterWorkbenchFeatures(features, query), [features, query])
  const normalizedQuery = query.trim()

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  return createPortal(
    <div
      className="fixed inset-0 z-[260] flex items-start justify-center bg-black/45 px-5 pt-[16vh] dark:bg-black/60"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        className="flex max-h-[62vh] w-full max-w-[560px] flex-col overflow-hidden rounded-xl border border-neutral-200 bg-white shadow-2xl shadow-black/25 dark:border-neutral-700 dark:bg-[#242426]"
        role="dialog"
        aria-modal="true"
        aria-label={t.workbenchSearchFeatures}
      >
        <div className="flex items-center gap-2 border-b border-neutral-200/80 px-3 py-2 dark:border-neutral-700/80">
          <Search size={15} strokeWidth={1.75} className="shrink-0 text-neutral-400" />
          <input
            ref={inputRef}
            type="search"
            value={query}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && results[0]) {
                if (event.nativeEvent.isComposing || event.keyCode === 229) return
                event.preventDefault()
                onSelect(results[0].page)
              }
            }}
            placeholder={t.workbenchSearchFeatures}
            className="min-w-0 flex-1 bg-transparent text-[14px] font-medium text-neutral-900 outline-none placeholder:text-neutral-400 dark:text-neutral-100 dark:placeholder:text-neutral-500"
          />
        </div>

        <div className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
          {normalizedQuery ? t.chatSearchResults : t.workbenchSearchAll}
        </div>

        <div className="custom-scrollbar min-h-0 overflow-y-auto px-1.5 pb-1.5">
          {results.length > 0 ? (
            results.map((item) => {
              const active = activePage === workbenchNavItem(item.page)
              return (
                <button
                  key={item.page}
                  type="button"
                  onClick={() => onSelect(item.page)}
                  title={item.group ? `${item.group} · ${item.label}` : item.label}
                  className={`flex w-full min-w-0 items-center gap-2 rounded-md px-2.5 py-1.5 text-left transition-colors ${
                    active
                      ? 'bg-black/[0.07] dark:bg-white/[0.1]'
                      : 'hover:bg-black/[0.04] dark:hover:bg-white/[0.07]'
                  }`}
                >
                  <span className={`min-w-0 flex-1 truncate text-[13px] font-medium ${
                    active
                      ? 'text-neutral-950 dark:text-neutral-50'
                      : 'text-neutral-800 dark:text-neutral-200'
                  }`}
                  >
                    {normalizedQuery ? (
                      <HighlightText text={item.label} query={normalizedQuery} />
                    ) : item.label}
                  </span>
                  {item.group ? (
                    <span className="max-w-[120px] shrink-0 truncate text-[12px] text-neutral-400 dark:text-neutral-500">
                      {item.group}
                    </span>
                  ) : null}
                </button>
              )
            })
          ) : (
            <div className="px-3 py-6 text-center text-[13px] text-neutral-400 dark:text-neutral-500">
              {t.workbenchSearchNoResults}
            </div>
          )}
        </div>
      </div>
    </div>,
    document.body,
  )
}
