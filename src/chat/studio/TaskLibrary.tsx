import { useEffect, useMemo, useRef, useState } from 'react'
import { ChevronLeft, ChevronRight, FolderOpen, Pin, RefreshCw, Search, Trash2, X } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import { selectLibraryTasks, taskFilters, type LibraryTask, type TaskFilter, type TaskSort } from './taskLibraryModel'
import type { TaskLibraryState } from './useTaskLibrary'
import { TaskDeleteDialog } from './TaskDeleteDialog'
import './taskLibrary.css'

const PAGE_SIZE = 24
export function TaskLibrary({ tasks, library, loading, disabled, currentId, onOpen, onRefresh, onNew, noun, onReveal, onDelete }: {
  onReveal: (id: string) => Promise<void>; onDelete: (id: string) => Promise<void>
  tasks: LibraryTask[]; library: TaskLibraryState; loading: boolean; disabled?: boolean; currentId?: string
  onOpen: (id: string) => void; onRefresh: () => Promise<void>; onNew: () => void; noun: '图片' | '视频'
}) {
  const [filter, setFilter] = useState<TaskFilter>('all')
  const [query, setQuery] = useState('')
  const [kind, setKind] = useState('')
  const [sort, setSort] = useState<TaskSort>('newest')
  const [page, setPage] = useState(0)
  const [selection, setSelection] = useState<string[]>([])
  const [refreshing, setRefreshing] = useState(false)
  const [refreshError, setRefreshError] = useState('')
  const [acting, setActing] = useState(false)
  const revealing = useRef(new Set<string>())
  const [revealingIds, setRevealingIds] = useState<string[]>([])
  const [actionError, setActionError] = useState('')
  const [deleteTarget, setDeleteTarget] = useState<LibraryTask[]>([])
  const [notice, setNotice] = useState('')
  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(''), 3500)
    return () => window.clearTimeout(timer)
  }, [notice])
  const requestDelete = (targets: LibraryTask[]) => {
    setNotice(''); setActionError(''); setDeleteTarget(targets)
  }
  const kinds = useMemo(() => [...new Map(tasks.map(t => [t.kind, t.kindLabel])).entries()], [tasks])
  const visible = useMemo(() => selectLibraryTasks(tasks, library.organization, filter, query, kind, sort), [tasks, library.organization, filter, query, kind, sort])
  const pages = Math.max(1, Math.ceil(visible.length / PAGE_SIZE))
  const currentPage = Math.min(page, pages - 1)
  const rows = visible.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE)
  const selected = rows.filter(t => selection.includes(t.id))
  const busy = disabled || library.pending || acting
  const editable = !busy
  const counts = useMemo(() => Object.fromEntries(taskFilters.map(f => [f.id, tasks.filter(t => f.id === 'all' || t.group === f.id).length])), [tasks])
  const reset = () => { setPage(0); setSelection([]); setNotice('') }
  const deleteTasks = async () => {
    if (busy || deleteTarget.some(t => !t.canDelete)) return
    setActing(true); setActionError(''); setNotice('')
    let removed = 0
    try {
      for (const task of deleteTarget) {
        await onDelete(task.id)
        removed++
        setSelection(ids => ids.filter(id => id !== task.id))
        setDeleteTarget(tasks => tasks.filter(t => t.id !== task.id))
      }
    } catch (e) { setActionError(`${removed ? `已删除 ${removed} 个任务，剩余任务` : '删除'}未完成：${String(e)}`) }
    finally {
      if (removed === deleteTarget.length) setNotice(`已删除 ${removed} 个任务及其专属素材和生成文件。`)
      setActing(false)
    }
  }
  const rowAction = async (id: string) => {
    if (revealing.current.has(id)) return
    revealing.current.add(id)
    setRevealingIds([...revealing.current]); setActionError('')
    try { await onReveal(id) }
    catch (e) { setActionError(String(e)) }
    finally { revealing.current.delete(id); setRevealingIds([...revealing.current]) }
  }
  const refresh = async () => {
    setRefreshing(true); setRefreshError('')
    try { await Promise.all([onRefresh(), library.refresh()]) }
    catch (e) { setRefreshError(String(e)) }
    finally { setRefreshing(false) }
  }
  return <section className="task-library" aria-label={`${noun}任务管理`}>
    <header className="tl-heading">
      <div><h2>{noun}任务 <span>{counts.all}</span></h2><p>聊天和工作台的制作记录都在这里，打开任务即可继续。</p></div>
      <div className="tl-actions">
        <IconButton label="刷新任务" disabled={refreshing || loading || busy} onClick={() => void refresh()}><RefreshCw size={16} className={refreshing ? 'tl-spin' : ''} /></IconButton>
        <Button size="sm" variant="primary" disabled={disabled} onClick={onNew}>新建{noun}</Button>
      </div>
    </header>
    <nav className="tl-filters" aria-label="任务状态筛选">
      {taskFilters.map(f => <button key={f.id} type="button" aria-pressed={filter === f.id} onClick={() => { setFilter(f.id); reset() }}>{f.label}<span>{counts[f.id]}</span></button>)}
    </nav>
    <div className="tl-toolbar">
      <label className="tl-search"><Search size={16} /><input aria-label="搜索任务" placeholder="搜索名称、要求或任务编号" value={query} onChange={e => { setQuery(e.target.value); reset() }} />{query && <IconButton size="xs" label="清空搜索" onClick={() => { setQuery(''); reset() }}><X size={13} /></IconButton>}</label>
      <select aria-label="任务类型" value={kind} onChange={e => { setKind(e.target.value); reset() }}><option value="">全部类型</option>{kinds.map(([id, label]) => <option value={id} key={id}>{label}</option>)}</select>
      <select aria-label="任务排序" value={sort} onChange={e => { setSort(e.target.value as TaskSort); reset() }}><option value="newest">最近更新</option><option value="oldest">最早更新</option><option value="name">名称排序</option></select>
    </div>
    {(library.error || refreshError) && <div className="tl-message tl-error" role="alert">{library.error || refreshError}<Button size="sm" onClick={() => void refresh()} disabled={refreshing}>重试</Button></div>}
    {actionError && !deleteTarget.length && <p className="tl-message tl-error" role="alert">{actionError}</p>}
    {deleteTarget.length > 0 && <TaskDeleteDialog count={deleteTarget.length} name={deleteTarget.length === 1 ? deleteTarget[0].name : undefined} busy={!!busy} error={actionError} onCancel={() => { setDeleteTarget([]); setActionError('') }} onConfirm={() => void deleteTasks()} />}
    {notice && <div className="tl-feedback" role="status"><span>{notice}</span><IconButton label="关闭提示" size="xs" onClick={() => setNotice('')}><X size={12} /></IconButton></div>}
    {selected.length > 0 && <div className="tl-bulk"><span>已选 {selected.length} 项</span><Button size="sm" variant="danger" disabled={!editable || selected.some(t => !t.canDelete)} onClick={() => { requestDelete(selected) }}><Trash2 size={14} />删除任务</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => setSelection([])}>取消选择</Button>{selected.some(t => !t.canDelete) && <small>进行中或待核查任务需先处理</small>}</div>}
    {loading && !tasks.length ? <p className="tl-empty" role="status">正在加载任务…</p> : !rows.length ? <div className="tl-empty"><FolderOpen size={30} /><h3>{tasks.length ? '没有符合条件的任务' : `还没有${noun}任务`}</h3><p>{tasks.length ? '试试其他状态，或清空搜索条件。' : `开始一次${noun}创作，保存后的任务会出现在这里。`}</p>{tasks.length > 0 && <Button size="sm" onClick={() => { setFilter('all'); setKind(''); setQuery(''); reset() }}>清空筛选</Button>}</div> : <>
      <div className="tl-list-head"><label><input type="checkbox" aria-label="选择本页任务" checked={rows.length > 0 && selected.length === rows.length} disabled={!editable} onChange={e => setSelection(e.target.checked ? rows.map(t => t.id) : [])} /><span>任务 · {visible.length} 项</span></label><span>状态 / 更新时间</span></div>
      <ul className="tl-list">{rows.map(t => <li key={t.id} className={`${t.id === currentId ? 'tl-current' : ''} ${selection.includes(t.id) ? 'tl-selected' : ''}`}>
        <input type="checkbox" aria-label={`选择 ${t.name}`} checked={selection.includes(t.id)} disabled={!editable} onChange={e => setSelection(ids => e.target.checked ? [...ids, t.id] : ids.filter(id => id !== t.id))} />
        <button type="button" className="tl-task" disabled={disabled} onClick={() => onOpen(t.id)} aria-label={`打开任务 ${t.name}`}>
          <span className="tl-task-main"><strong>{library.organization[t.id]?.pinned && <Pin size={12} />}{t.name}</strong><span className="tl-description">{t.description || t.kindLabel}</span><small>{t.kindLabel}{t.detail && ` · ${t.detail}`}</small>{t.error && <span className="tl-task-error">{t.error}</span>}</span>
          <span className="tl-task-state"><span className={`tl-status tl-${t.group}`}><i />{t.status}</span><time dateTime={t.updatedAt ? new Date(t.updatedAt).toISOString() : undefined}>{t.updatedAt ? new Date(t.updatedAt).toLocaleString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' }) : '时间未知'}</time></span>
        </button>
        <div className="tl-row-actions"><IconButton label={`打开所在文件夹 ${t.name}`} disabled={busy || revealingIds.includes(t.id)} onClick={() => void rowAction(t.id)}><FolderOpen size={14} /></IconButton><IconButton variant="danger" label={`删除 ${t.name}`} disabled={busy || !t.canDelete} onClick={() => { requestDelete([t]) }}><Trash2 size={14} /></IconButton></div>
      </li>)}</ul>
      <footer className="tl-pagination"><span>第 {currentPage * PAGE_SIZE + 1}–{Math.min((currentPage + 1) * PAGE_SIZE, visible.length)} 项，共 {visible.length} 项</span><div><IconButton label="上一页" disabled={currentPage === 0} onClick={() => { setPage(currentPage - 1); setSelection([]) }}><ChevronLeft size={16} /></IconButton><span>{currentPage + 1} / {pages}</span><IconButton label="下一页" disabled={currentPage + 1 >= pages} onClick={() => { setPage(currentPage + 1); setSelection([]) }}><ChevronRight size={16} /></IconButton></div></footer>
    </>}
  </section>
}
