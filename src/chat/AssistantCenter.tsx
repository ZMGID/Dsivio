import { useCallback, useEffect, useState } from 'react'
import { ArrowLeft, Check, Clapperboard, ImageIcon, Pencil, Plus, Search, Sparkles, Star, Trash2 } from 'lucide-react'
import { api } from '../api/tauri'
import { Button, IconButton } from '../components/Button'
import { chatApi } from './api'
import { AssistantEditor } from './AssistantEditor'
import {
  ASSISTANT_PROMPT_CATEGORIES,
  ASSISTANT_PROMPT_CATEGORY_LABELS,
  assistantPromptCategory,
  type AssistantPromptCategoryFilter,
} from './assistantCategories'
import type { ChatAssistant, SkillMeta } from './types'

interface AssistantCenterProps {
  skills: SkillMeta[]
  currentAssistantId?: string | null
  onStartAssistantChat: (assistant: ChatAssistant) => void
  onStartBuilder?: () => void
  onApplyAssistant?: (assistantId: string | null) => void
}

export function AssistantCenter({ currentAssistantId, onStartAssistantChat, onApplyAssistant }: AssistantCenterProps) {
  const [assistants, setAssistants] = useState<ChatAssistant[]>([])
  const [editing, setEditing] = useState<ChatAssistant | null | undefined>(undefined)
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<AssistantPromptCategoryFilter>('all')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const [deleting, setDeleting] = useState<string | null>(null)
  const [savingFavorite, setSavingFavorite] = useState<string | null>(null)
  const load = useCallback(async () => {
    try { setAssistants((await chatApi.getAssistants()).filter(a => !a.archived)); setError('') }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setLoading(false) }
  }, [])
  useEffect(() => {
    void load()
    const subscription = api.onChatAssistantsChanged(load)
    return () => { void subscription.then(unlisten => unlisten()) }
  }, [load])
  const remove = async (assistant: ChatAssistant) => {
    if (!window.confirm(`删除助手“${assistant.name}”？已有对话和生成结果会保留。`)) return
    setDeleting(assistant.id)
    try { await chatApi.deleteAssistant(assistant.id); await load() }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setDeleting(null) }
  }
  const needle = query.trim().toLocaleLowerCase()
  const filtered = assistants.filter(a => {
    if (category === 'favorite' && !a.installed) return false
    if (category !== 'all' && category !== 'favorite' && assistantPromptCategory(a) !== category) return false
    return `${a.name}\n${a.description ?? ''}\n${a.system_prompt ?? a.systemPrompt ?? ''}`.toLocaleLowerCase().includes(needle)
  })
  const categoryCount = (value: AssistantPromptCategoryFilter) => value === 'all'
    ? assistants.length
    : value === 'favorite'
      ? assistants.filter(a => a.installed).length
      : assistants.filter(a => assistantPromptCategory(a) === value).length
  const visibleCategories = (['all', 'favorite', ...ASSISTANT_PROMPT_CATEGORIES] as AssistantPromptCategoryFilter[])
    .filter(value => value === 'all' || value === 'favorite' || categoryCount(value) > 0 || category === value)
  const groups = ASSISTANT_PROMPT_CATEGORIES
    .map(value => ({
      category: value,
      assistants: filtered.filter(assistant => assistantPromptCategory(assistant) === value),
    }))
    .filter(group => group.assistants.length > 0)
  const handleUseAssistant = (assistant: ChatAssistant) => {
    if (onApplyAssistant) onApplyAssistant(assistant.id)
    else onStartAssistantChat(assistant)
  }
  const toggleFavorite = async (assistant: ChatAssistant) => {
    setSavingFavorite(assistant.id)
    try {
      await chatApi.updateAssistant({
        ...assistant,
        installed: !assistant.installed,
        updated_at: Math.floor(Date.now() / 1000),
      })
      await load()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSavingFavorite(null)
    }
  }
  const categoryIcon = {
    image: ImageIcon,
    video: Clapperboard,
    general: Sparkles,
  }
  return <main className="assistant-center-root custom-scrollbar h-full overflow-y-auto p-4 text-neutral-900 dark:text-neutral-100 sm:p-6">
    <div className="mx-auto max-w-5xl space-y-5">
      <header className="flex flex-col items-start gap-3 pb-1 sm:flex-row sm:justify-between sm:gap-4">
        <div><div className="flex items-baseline gap-2">
          <h1 className="text-xl font-semibold tracking-tight">{editing === undefined ? '提示词助手' : editing ? '编辑助手' : '新建助手'}</h1>
          {editing === undefined && !loading && <span className="text-xs text-neutral-400">{assistants.length} 个</span>}
        </div>
          <p className="mt-1 text-sm text-neutral-500">保存图片和视频的专业提示词，需要时直接使用。</p></div>
        {editing === undefined ? <Button className="self-stretch sm:self-auto" variant="primary" onClick={() => setEditing(null)}><Plus size={15} />新建助手</Button>
          : <Button variant="ghost" onClick={() => { setEditing(undefined); void load() }}><ArrowLeft size={15} />返回列表</Button>}
      </header>
      {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
      {editing !== undefined ? <AssistantEditor key={editing?.id ?? 'new'} assistant={editing}
        onCancel={() => { setEditing(undefined); void load() }} onSaved={() => { setEditing(undefined); void load() }} /> : <>
        <div className="flex flex-col gap-3 border-b border-neutral-200 pb-4 dark:border-neutral-800 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex w-fit rounded-lg bg-neutral-100 p-1 dark:bg-neutral-900" role="tablist" aria-label="助手分类">
          {visibleCategories.map(value => (
            <button key={value} type="button" role="tab" aria-selected={category === value}
              className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${category === value
                ? 'bg-white text-neutral-900 shadow-sm dark:bg-neutral-700 dark:text-white'
                : 'text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100'}`}
              onClick={() => setCategory(value)}>
              {value === 'all' ? '全部' : value === 'favorite' ? '收藏' : ASSISTANT_PROMPT_CATEGORY_LABELS[value]}
              <span className="ml-1.5 tabular-nums text-neutral-400">{categoryCount(value)}</span>
            </button>
          ))}
          </div>
          {assistants.length > 0 && <label className="relative block w-full sm:w-64">
            <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-neutral-400" size={14} />
            <input className="kv-input h-8 w-full" style={{ paddingLeft: '2rem' }} aria-label="搜索助手" placeholder="搜索助手" value={query} onChange={e => setQuery(e.target.value)} />
          </label>}
        </div>
        {loading ? <p className="py-12 text-center text-sm text-neutral-500">正在读取助手…</p>
          : filtered.length === 0 ? <div className="rounded-xl border border-dashed border-neutral-200 px-6 py-16 text-center dark:border-neutral-800">
            <p className="font-medium">{needle ? '没有找到匹配的助手' : category === 'favorite' ? '还没有收藏助手' : '把你精选的提示词存成助手'}</p>
            <p className="mt-2 text-sm text-neutral-500">{needle ? '试试其他关键词。' : category === 'favorite'
              ? '点击助手右侧的星标，收藏后会出现在这里和聊天快捷列表。'
              : '只需名称和系统提示词。图片、视频和聊天都能使用。'}</p>
            {!needle && category !== 'favorite' && <Button className="mt-5" onClick={() => setEditing(null)}><Plus size={15} />创建第一个助手</Button>}
          </div> : <div className="space-y-6">
            {groups.map(group => {
              const CategoryIcon = categoryIcon[group.category]
              return <section key={group.category} aria-labelledby={`assistant-group-${group.category}`}>
                <div className="mb-2 flex items-center gap-2 px-1">
                  <CategoryIcon size={14} className="text-neutral-400" aria-hidden="true" />
                  <h2 id={`assistant-group-${group.category}`} className="text-xs font-semibold text-neutral-600 dark:text-neutral-300">
                    {ASSISTANT_PROMPT_CATEGORY_LABELS[group.category]}
                  </h2>
                  <span className="text-[11px] tabular-nums text-neutral-400">{group.assistants.length}</span>
                </div>
                <div className="grid gap-2 lg:grid-cols-2">
                  {group.assistants.map(a => {
                    const active = currentAssistantId === a.id
                    return <article key={a.id} className="group flex min-h-[92px] min-w-0 items-center gap-3 rounded-xl border border-neutral-200 bg-white px-4 py-3.5 transition-colors hover:border-neutral-300 hover:bg-neutral-50 dark:border-neutral-800 dark:bg-neutral-950/30 dark:hover:border-neutral-700 dark:hover:bg-neutral-900/60">
                      <div className="hidden h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-neutral-100 text-neutral-500 dark:bg-neutral-800 dark:text-neutral-300 sm:flex">
                        <CategoryIcon size={18} aria-hidden="true" />
                      </div>
                      <button type="button" className="min-w-0 flex-1 text-left" onClick={() => setEditing(a)} aria-label={`编辑 ${a.name}`}>
                        <h3 className="truncate text-sm font-medium">{a.name}</h3>
                        <p className="mt-1 line-clamp-2 text-xs leading-relaxed text-neutral-500">{a.description || '暂无描述，点击名称补充这个助手的用途。'}</p>
                      </button>
                      <div className="flex shrink-0 items-center gap-0.5">
                        <IconButton size="sm" variant="ghost" label={`${a.installed ? '取消收藏' : '收藏'} ${a.name}`}
                          disabled={savingFavorite !== null} onClick={() => void toggleFavorite(a)}
                          className={a.installed ? 'text-amber-500!' : ''}>
                          <Star size={14} fill={a.installed ? 'currentColor' : 'none'} />
                        </IconButton>
                        <div className="hidden opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 sm:flex">
                          <IconButton size="sm" label={`编辑 ${a.name}`} onClick={() => setEditing(a)}><Pencil size={14} /></IconButton>
                          <IconButton size="sm" variant="danger" label={`删除 ${a.name}`} disabled={deleting !== null} onClick={() => void remove(a)}><Trash2 size={14} /></IconButton>
                        </div>
                        <Button size="sm" variant={active ? 'ghost' : 'default'} disabled={active} onClick={() => handleUseAssistant(a)}>
                          {active && <Check size={13} />}{active ? '已使用' : '使用'}
                        </Button>
                      </div>
                    </article>
                  })}
                </div>
              </section>
            })}
          </div>}
      </>}
    </div>
  </main>
}
