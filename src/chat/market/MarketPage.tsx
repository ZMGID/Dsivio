import { marketText, useMarketAction, actionLabel } from './marketActions'
import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, Film, Image, LayoutGrid, Loader2, MoreHorizontal, RefreshCw, Search } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import type { Lang } from '../../components/i18n'
import { Input } from '../../settings/public/controls'
import { marketApi, useMarket } from './api'
import { marketItems, primaryAction, type MarketItem, type MarketLocal, type MarketExample } from './types'
import './market.css'

export type MarketActions = { onInstall: (id: string) => Promise<void>; onUse: (local: MarketLocal, newChat: boolean) => Promise<void>; onUninstall: (id: string) => Promise<void> }
const viewState = { query: '', scope: 'public' as 'public' | 'personal', scroll: 0 }
function matchesQuery(item: MarketItem, query: string) {
  return `${item.manifest?.name ?? item.id} ${item.manifest?.summary ?? ''}`.toLowerCase().includes(query.trim().toLowerCase())
}
function inPublicCatalog(item: MarketItem) {
  if (item.entry) return true
  const source = item.local?.source
  if (!source) return false
  return source.kind === 'local-draft' || source.kind === 'built-in' || 'repository' in source
}
function unfinishedInstall(item: MarketItem) {
  return Boolean(item.local && item.local.source?.kind !== 'built-in' && item.local.status !== 'ready' && !item.local.pluginId)
}
function statusLabel(item: MarketItem, lang: Lang) {
  if (!item.local) return ''
  return marketText(lang, item.local.phase === 'removing' ? '卸载待完成' : item.local.status === 'ready' ? '已安装' : item.local.status === 'failed' ? '需修复' : '安装待完成', item.local.phase === 'removing' ? 'Removal pending' : item.local.status === 'ready' ? 'Installed' : item.local.status === 'failed' ? 'Needs repair' : 'Setup pending')
}
export function PackageIcon({ categories, item }: { categories?: string[]; item?: MarketItem }) {
  const [src, setSrc] = useState('')
  const id = item?.id
  const icon = item?.manifest?.icon
  const source = item?.local?.source ?? item?.entry?.source
  const revision = source && 'revision' in source ? source.revision : undefined
  useEffect(() => {
    let disposed = false
    setSrc('')
    if (id && icon) void marketApi.icon(id).then(url => { if (!disposed) setSrc(url) }).catch(() => {})
    return () => { disposed = true }
  }, [id, icon, revision])
  const iconClass = `market-icon${id === 'hypit' ? ' market-icon-hypit' : ''}`
  if (src) return <span className={iconClass}><img src={src} alt="" onError={() => setSrc('')} style={{ width: '100%', height: '100%', objectFit: 'contain' }} /></span>
  return <span className={iconClass}>{categories?.includes('videos') ? <Film size={20} /> : categories?.includes('images') ? <Image size={20} /> : <LayoutGrid size={20} />}</span>
}
export function LoadSwitch({ item, busy, onChange, lang }: { item: MarketItem; busy: boolean; onChange: () => void; lang: Lang }) {
  if (item.local?.status !== 'ready' || item.local.source?.kind === 'built-in') return null
  const enabled = item.local.enabled
  return <span className="market-load"><span>{marketText(lang, enabled ? '已加载' : '未加载', enabled ? 'Loaded' : 'Not loaded')}</span><button className="market-switch" type="button" role="switch" aria-checked={enabled} aria-label={`${item.manifest?.name ?? item.id} ${marketText(lang, '加载', 'load')}`} disabled={busy} onClick={onChange}><span /></button></span>
}
function detailId() {
  const match = window.location.hash.match(/^#chat\/market\/([^/?]+)/)
  if (!match) return null
  try { return decodeURIComponent(match[1]) } catch { return null }
}
function Example({ id, lang, local }: { id: string; lang: Lang; local: boolean }) {
  const [detail, setDetail] = useState<{ example: MarketExample; assetBase: string } | null>(null)
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  useEffect(() => {
    let disposed = false; setDetail(null); setError('')
    void marketApi.detail(id, local).then(value => { if (!disposed) setDetail(value) }).catch(e => { if (!disposed) setError(String(e)) })
    return () => { disposed = true }
  }, [id, local, retry])
  if (error) return <div className="market-notice">{marketText(lang, '示例暂时无法加载', 'Example unavailable')} <Button size="sm" onClick={() => setRetry(n => n + 1)}>{marketText(lang, '重试', 'Retry')}</Button></div>
  if (!detail) return <div className="market-skeleton" aria-label={marketText(lang, '正在读取示例', 'Loading example')} />
  return <div className="market-example">{detail.example.messages.map((message, index) => <div key={index} className={`market-example-message ${message.role}`}><span className="market-message-role">{message.role === 'user' ? marketText(lang, '你', 'You') : 'AI'}</span><div className="market-bubble">{message.attachments?.map((attachment, i) => {
    const url = `${detail.assetBase}${attachment.path}`
    return <div className="market-example-attachment" key={i}>{attachment.type === 'image' ? <a href={url} target="_blank" rel="noreferrer"><img loading="lazy" src={url} alt={attachment.label} /></a> : attachment.type === 'video' ? <video controls preload="none" poster={attachment.poster ? `${detail.assetBase}${attachment.poster}` : undefined} src={url} aria-label={attachment.label} /> : <span>{attachment.label}</span>}</div>
  })}<p>{message.text}</p></div></div>)}</div>
}
export function MarketPage({ lang, onInstall, onUse, onUninstall }: MarketActions & { lang: Lang }) {
  const { snapshot, loading, initialLoading } = useMarket()
  const items = useMemo(() => marketItems(snapshot), [snapshot])
  const [query, setQuery] = useState(viewState.query)
  const [scope, setScope] = useState(viewState.scope)
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({})
  const [selected, setSelected] = useState(detailId)
  const [menuId, setMenuId] = useState<string | null>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const { busyIds, error, run, use } = useMarketAction({ onInstall, onUse })
  useEffect(() => { const change = () => { setSelected(detailId()); setMenuId(null) }; window.addEventListener('hashchange', change); return () => window.removeEventListener('hashchange', change) }, [])
  useEffect(() => { Object.assign(viewState, { query, scope }) }, [query, scope])
  useEffect(() => { if (!selected && scroller.current) scroller.current.scrollTop = viewState.scroll }, [selected])
  useEffect(() => {
    const close = (e: KeyboardEvent) => { if (e.key === 'Escape') { setMenuId(null) } }
    window.addEventListener('keydown', close); return () => window.removeEventListener('keydown', close)
  }, [])
  const chosen = items.find(item => item.id === selected)
  const categories = snapshot.categories
  const installed = items.filter(item => item.local?.status === 'ready' && matchesQuery(item, query))
  const listed = items.filter(item => (scope === 'public' ? inPublicCatalog(item) : Boolean(item.local) && !inPublicCatalog(item)) && matchesQuery(item, query))
  const sections = useMemo(() => {
    const buckets = new Map<string, MarketItem[]>()
    for (const item of listed) {
      const id = item.manifest?.categoryIds[0] || 'other'
      buckets.set(id, [...(buckets.get(id) ?? []), item])
    }
    const ordered = categories.flatMap(category => {
      const group = buckets.get(category.id)
      if (!group?.length) return []
      buckets.delete(category.id)
      return [{ id: category.id, name: category.name, items: group }]
    })
    for (const [id, group] of buckets) ordered.push({ id, name: id === 'other' ? marketText(lang, '其他', 'Other') : id, items: group })
    return ordered
  }, [listed, categories, lang])
  const openDetail = (id: string) => { window.location.hash = `#chat/market/${encodeURIComponent(id)}` }
  const toggle = (item: MarketItem) => void run(item.id, () => marketApi.setEnabled(item.id, !item.local?.enabled))
  const remove = (item: MarketItem) => unfinishedInstall(item) ? marketApi.discard(item.id) : onUninstall(item.id)
  const more = (item: MarketItem) => item.local && item.local.phase !== 'removing' && <div className="market-more"><IconButton label={marketText(lang, `${item.manifest?.name} 更多操作`, `More options for ${item.manifest?.name}`)} onClick={() => setMenuId(menuId === item.id ? null : item.id)}><MoreHorizontal size={16} /></IconButton>{menuId === item.id && <div className="market-more-menu"><Button size="sm" variant="danger" disabled={busyIds.has(item.id)} onClick={() => { setMenuId(null); void run(item.id, () => remove(item)) }}>{marketText(lang, '卸载', 'Uninstall')}</Button></div>}</div>
  const actionButton = (item: MarketItem, variant: 'default' | 'primary' = 'default') => item.local?.phase === 'removing'
    ? <Button variant={variant} size="sm" disabled={busyIds.has(item.id)} onClick={() => void run(item.id, () => onUninstall(item.id))}>{marketText(lang, '继续卸载', 'Continue removal')}</Button>
    : <Button variant={variant} size="sm" disabled={busyIds.has(item.id) || primaryAction(item) === 'unavailable'} onClick={() => void use(item)}>{busyIds.has(item.id) && <Loader2 size={14} className="animate-spin" />}{actionLabel(item, lang)}</Button>
  return <section className="market-page" data-tauri-drag-region="false">
    <div className="market-scroll custom-scrollbar" ref={scroller} onScroll={e => { if (!selected) viewState.scroll = e.currentTarget.scrollTop }}>
      {selected ? <>
        <Button variant="ghost" size="sm" onClick={() => { window.location.hash = '#chat/market' }}><ArrowLeft size={15} />{marketText(lang, '返回插件', 'Back to plugins')}</Button>
        {!chosen?.manifest ? <div className="market-empty">{loading ? marketText(lang, '正在读取应用…', 'Loading app…') : marketText(lang, '这个应用暂时无法查看', 'This app is unavailable')}</div> : <div className="market-detail">
          <header className="market-detail-heading"><PackageIcon item={chosen} categories={chosen.manifest.categoryIds} /><div><h1>{chosen.manifest.name}</h1><p>{chosen.manifest.summary}</p></div>{more(chosen)}</header>
          {chosen.entry?.source.kind === 'built-in' ? <>
            {chosen.local && <p className="market-status">{statusLabel(chosen, lang)}</p>}
            {chosen.manifest.setupSkillId && <section className="market-components"><h2>{marketText(lang, '技能', 'Skills')} {1 + (chosen.manifest.skillIds?.length ?? 1)}</h2>
              <div><strong>{chosen.manifest.setupSkillId}</strong><span>{marketText(lang, '检查 Dsivio 接入并补齐运行环境', 'Check Dsivio integration and fill missing dependencies')}</span></div>
              {(chosen.manifest.skillIds ?? [chosen.manifest.mainSkillId]).filter((skill): skill is string => Boolean(skill)).map(skill => <div key={skill}><strong>{skill}</strong><span>{marketText(lang, '安装后即可使用', 'Available after installation')}</span></div>)}
            </section>}
            {chosen.manifest.checkCommand && <section className="market-components"><h2>{marketText(lang, '命令', 'Commands')} 1</h2><div><code>{chosen.manifest.checkCommand}</code><span>{marketText(lang, '检查接入、配置和实际可用性', 'Check integration, configuration, and runtime readiness')}</span></div></section>}
          </> : <>
          <h2>{marketText(lang, '使用示例', 'How to use')}</h2><p className="market-muted">{marketText(lang, '示例仅供说明，不会发送到你的对话。', 'This example will not be sent to your conversation.')}</p>
          <Example id={chosen.id} lang={lang} local={Boolean(chosen.local)} />
          {chosen.manifest.notices.map(notice => <p className="market-condition" key={notice}>{notice}</p>)}
          <details className="market-info"><summary>{marketText(lang, '详细信息', 'Details')}</summary><dl><dt>{marketText(lang, '安装版本', 'Installed version')}</dt><dd>{chosen.local?.manifest.version ?? '—'}</dd><dt>{chosen.entry?.source.kind === 'local-draft' ? marketText(lang, '试用版本', 'Trial version') : marketText(lang, '发布版本', 'Published version')}</dt><dd>{chosen.entry?.version ?? '—'}</dd><dt>{marketText(lang, '支持系统', 'Platforms')}</dt><dd>{chosen.manifest.compatibility.platforms.join(' · ')}</dd><dt>{marketText(lang, '最近验证', 'Last verified')}</dt><dd>{chosen.manifest.verification.map(v => `${v.platform}: ${v.verifiedAt}`).join(' · ') || marketText(lang, '等待试跑验证', 'Awaiting trial verification')}</dd></dl></details>
          </>}
        </div>}
      </> : <>
        <header className="market-heading">
          <div className="min-w-0"><h1>{marketText(lang, '插件市场', 'Plugin market')}</h1><p>{marketText(lang, '用插件扩展对话里的技能与能力。', 'Use plugins to extend what chat can do.')}</p></div>
          <IconButton label={marketText(lang, '刷新', 'Refresh')} disabled={loading} onClick={() => void marketApi.refresh()}><RefreshCw size={16} className={loading ? 'animate-spin' : ''} /></IconButton>
        </header>
        <label className="market-search"><Search size={16} /><Input value={query} onChange={setQuery} placeholder={marketText(lang, '搜索插件', 'Search plugins')} aria-label={marketText(lang, '搜索插件', 'Search plugins')} /></label>
        <section className="market-installed">
          <h2>{marketText(lang, '已安装', 'Installed')}</h2>
          {installed.length ? <div className="market-installed-row custom-scrollbar">{installed.map(item => <button type="button" key={item.id} className={`market-installed-item${item.id === 'hypit' ? ' market-installed-hypit' : ''}`} aria-label={item.manifest?.name ?? item.id} onClick={() => openDetail(item.id)}><PackageIcon item={item} categories={item.manifest?.categoryIds} /></button>)}</div> : <p className="market-muted">{marketText(lang, query.trim() ? '没有找到相关插件' : '还没有安装插件', query.trim() ? 'No matching plugins' : 'No plugins installed yet')}</p>}
        </section>
        <div className="market-scope" role="tablist" aria-label={marketText(lang, '插件范围', 'Plugin scope')}>
          <button type="button" role="tab" aria-selected={scope === 'public'} onClick={() => setScope('public')}>{marketText(lang, '公开', 'Public')}</button>
          <button type="button" role="tab" aria-selected={scope === 'personal'} onClick={() => setScope('personal')}>{marketText(lang, '个人', 'Personal')}</button>
        </div>
        {snapshot.preview && <p className="market-muted">{marketText(lang, '浏览器预览 · 安装和使用插件请打开 dsivio 桌面端。', 'Browser preview · Install and use plugins in the dsivio desktop app.')}</p>}
        {snapshot.error && <div role="status" className="market-notice">{snapshot.refreshedAt ? marketText(lang, '暂时无法刷新，已显示上次内容。', 'Refresh failed. Showing cached content.') : marketText(lang, '暂时无法读取市场。', 'The market could not be loaded.')}<details><summary>{marketText(lang, '查看原因', 'Details')}</summary>{snapshot.error}</details><Button size="sm" onClick={() => void marketApi.refresh()}>{marketText(lang, '重试', 'Retry')}</Button></div>}
        {initialLoading && !items.length ? <div className="market-rows" aria-busy="true">{[0, 1, 2, 3].map(i => <div className="market-skeleton" key={i} />)}</div> : !sections.length ? <div className="market-empty"><LayoutGrid size={28} /><h2>{marketText(lang, query.trim() ? '没有找到相关插件' : scope === 'personal' ? '还没有安装插件' : snapshot.error ? '市场暂不可用' : '暂无插件', query.trim() ? 'No matching plugins' : scope === 'personal' ? 'No plugins installed yet' : snapshot.error ? 'Market unavailable' : 'No plugins yet')}</h2><p>{marketText(lang, query.trim() ? '试试其他关键词。' : '团队发布后会显示在这里，无需更新软件。', query.trim() ? 'Try another search.' : 'Published plugins will appear here without an app update.')}</p>{query.trim() && <Button size="sm" onClick={() => setQuery('')}>{marketText(lang, '查看全部', 'Show all')}</Button>}</div> : sections.map(section => <section className="market-section" key={section.id}>
          <h2>{section.name}</h2>
          {!collapsed[section.id] && <div className="market-rows">{section.items.map(item => <article className="market-row" key={item.id}><button type="button" className="market-row-info" onClick={() => openDetail(item.id)}><PackageIcon item={item} categories={item.manifest?.categoryIds} /><span className="min-w-0"><strong>{item.manifest?.name ?? item.id}</strong><span>{item.manifest?.summary ?? item.entry?.error}</span></span></button><span className="market-row-actions">{item.local && <span className="market-status">{statusLabel(item, lang)}</span>}{more(item)}{actionButton(item)}</span></article>)}</div>}
          <button type="button" className="market-collapse" aria-expanded={!collapsed[section.id]} onClick={() => setCollapsed(current => ({ ...current, [section.id]: !current[section.id] }))}>{marketText(lang, collapsed[section.id] ? '展开' : '收起', collapsed[section.id] ? 'Expand' : 'Collapse')}</button>
        </section>)}
      </>}
      {error && <p className="market-error" role="alert">{error}</p>}
    </div>
    {selected && chosen?.manifest && <footer className="market-detail-footer"><LoadSwitch item={chosen} busy={busyIds.has(chosen.id)} onChange={() => toggle(chosen)} lang={lang} /><span className="market-primary">{actionButton(chosen, 'primary')}</span></footer>}

  </section>
}
