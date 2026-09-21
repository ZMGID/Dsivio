import { marketText, useMarketAction, actionLabel } from './marketActions'
import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeft, Film, Image, LayoutGrid, Loader2, MoreHorizontal, RefreshCw, Search } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import type { Lang } from '../../components/i18n'
import { marketApi, useMarket } from './api'
import { marketItems, primaryAction, type MarketItem, type MarketLocal, type MarketExample } from './types'
import { CapabilityConfig } from './CapabilityConfig'
import './market.css'

export type MarketActions = { onInstall: (id: string) => Promise<void>; onUse: (local: MarketLocal, newChat: boolean) => Promise<void>; onUninstall: (id: string) => Promise<void> }
const viewState = { query: '', category: '', installedOnly: false, scroll: 0 }
export function PackageIcon({ categories, item }: { categories?: string[]; item?: MarketItem }) {
  const [src, setSrc] = useState('')
  const id = item?.id
  const icon = item?.manifest?.icon
  const revision = (item?.local?.source ?? item?.entry?.source)?.revision
  useEffect(() => {
    let disposed = false
    setSrc('')
    if (id && icon) void marketApi.icon(id).then(url => { if (!disposed) setSrc(url) }).catch(() => {})
    return () => { disposed = true }
  }, [id, icon, revision])
  if (src) return <span className="market-icon"><img src={src} alt="" onError={() => setSrc('')} style={{ width: '100%', height: '100%', objectFit: 'contain' }} /></span>
  return <span className="market-icon">{categories?.includes('videos') ? <Film size={20} /> : categories?.includes('images') ? <Image size={20} /> : <LayoutGrid size={20} />}</span>
}
export function LoadSwitch({ item, busy, onChange, lang }: { item: MarketItem; busy: boolean; onChange: () => void; lang: Lang }) {
  if (item.local?.status !== 'ready') return null
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
  const [category, setCategory] = useState(viewState.category)
  const [installedOnly, setInstalledOnly] = useState(viewState.installedOnly)
  const [selected, setSelected] = useState(detailId)
  const [configOpen, setConfigOpen] = useState(false)
  const [menuId, setMenuId] = useState<string | null>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const { busyIds, error, run, use } = useMarketAction({ onInstall, onUse })
  useEffect(() => { const change = () => { setSelected(detailId()); setMenuId(null) }; window.addEventListener('hashchange', change); return () => window.removeEventListener('hashchange', change) }, [])
  useEffect(() => { Object.assign(viewState, { query, category, installedOnly }) }, [query, category, installedOnly])
  useEffect(() => { if (!selected && scroller.current) scroller.current.scrollTop = viewState.scroll }, [selected])
  useEffect(() => {
    const close = (e: KeyboardEvent) => { if (e.key === 'Escape') { setMenuId(null) } }
    window.addEventListener('keydown', close); return () => window.removeEventListener('keydown', close)
  }, [])
  const chosen = items.find(item => item.id === selected)
  const categories = snapshot.categories
  const visible = items.filter(item => (!installedOnly || item.local) && (!category || item.manifest?.categoryIds.includes(category)) && `${item.manifest?.name ?? item.id} ${item.manifest?.summary ?? ''} ${categories.filter(c => item.manifest?.categoryIds.includes(c.id)).map(c => c.name).join(' ')}`.toLowerCase().includes(query.trim().toLowerCase()))
  const toggle = (item: MarketItem) => void run(item.id, () => marketApi.setEnabled(item.id, !item.local?.enabled))
  const more = (item: MarketItem) => item.local && <div className="market-more"><IconButton label={marketText(lang, `${item.manifest?.name} 更多操作`, `More options for ${item.manifest?.name}`)} onClick={() => setMenuId(menuId === item.id ? null : item.id)}><MoreHorizontal size={16} /></IconButton>{menuId === item.id && <div className="market-more-menu"><Button size="sm" variant="danger" disabled={busyIds.has(item.id)} onClick={() => { setMenuId(null); void run(item.id, () => onUninstall(item.id)) }}>{marketText(lang, '卸载', 'Uninstall')}</Button></div>}</div>
  const primary = (item: MarketItem) => <Button variant="primary" size="sm" disabled={busyIds.has(item.id) || primaryAction(item) === 'unavailable'} onClick={() => void use(item)}>{busyIds.has(item.id) && <Loader2 size={14} className="animate-spin" />}{actionLabel(item, lang)}</Button>
  return <section className="market-page" data-tauri-drag-region="false">
    <div className="market-scroll" ref={scroller} onScroll={e => { if (!selected) viewState.scroll = e.currentTarget.scrollTop }}>
      {selected ? <>
        <Button variant="ghost" size="sm" onClick={() => { window.location.hash = '#chat/market' }}><ArrowLeft size={15} />{marketText(lang, '返回应用市场', 'Back to apps')}</Button>
        {!chosen?.manifest ? <div className="market-empty">{loading ? marketText(lang, '正在读取应用…', 'Loading app…') : marketText(lang, '这个应用暂时无法查看', 'This app is unavailable')}</div> : <div className="market-detail">
          <header className="market-detail-heading"><PackageIcon item={chosen} categories={chosen.manifest.categoryIds} /><div><h1>{chosen.manifest.name}</h1><p>{chosen.manifest.summary}</p></div>{more(chosen)}</header>
          <h2>{marketText(lang, '使用示例', 'How to use')}</h2><p className="market-muted">{marketText(lang, '示例仅供说明，不会发送到你的对话。', 'This example will not be sent to your conversation.')}</p>
          <Example id={chosen.id} lang={lang} local={Boolean(chosen.local)} />
          {chosen.manifest.notices.map(notice => <p className="market-condition" key={notice}>{notice}</p>)}
          <details className="market-info"><summary>{marketText(lang, '详细信息', 'Details')}</summary><dl><dt>{marketText(lang, '安装版本', 'Installed version')}</dt><dd>{chosen.local?.manifest.version ?? '—'}</dd><dt>{chosen.entry?.source.kind === 'local-draft' ? marketText(lang, '试用版本', 'Trial version') : marketText(lang, '发布版本', 'Published version')}</dt><dd>{chosen.entry?.version ?? '—'}</dd><dt>{marketText(lang, '支持系统', 'Platforms')}</dt><dd>{chosen.manifest.compatibility.platforms.join(' · ')}</dd><dt>{marketText(lang, '最近验证', 'Last verified')}</dt><dd>{chosen.manifest.verification.map(v => `${v.platform}: ${v.verifiedAt}`).join(' · ') || marketText(lang, '等待试跑验证', 'Awaiting trial verification')}</dd></dl></details>
        </div>}
      </> : <>
        <header className="market-heading"><div><h1>{marketText(lang, '应用市场', 'App market')}</h1><p>{marketText(lang, '经过团队验证，选择需要的功能，在对话中使用。', 'Team-tested apps, ready to use in conversation.')}</p></div><div className="flex items-center gap-2"><Button size="sm" aria-haspopup="dialog" onClick={() => setConfigOpen(true)}>{marketText(lang, '能力配置', 'Capabilities')}</Button><Button size="sm" disabled={loading} onClick={() => void marketApi.refresh()}><RefreshCw size={14} className={loading ? 'animate-spin' : ''} />{marketText(lang, '刷新', 'Refresh')}</Button></div></header>
        <CapabilityConfig lang={lang} open={configOpen} onClose={() => setConfigOpen(false)} />
        <label className="market-search"><Search size={16} /><input value={query} onChange={e => setQuery(e.target.value)} placeholder={marketText(lang, '搜索应用名称或用途', 'Search apps or use cases')} aria-label={marketText(lang, '搜索应用', 'Search apps')} /></label>
        <div className="market-filters"><div role="group" aria-label={marketText(lang, '分类', 'Categories')}>{[{ id: '', name: marketText(lang, '全部', 'All') }, ...categories].map(c => <button type="button" key={c.id} aria-pressed={category === c.id} onClick={() => setCategory(c.id)}>{c.name}</button>)}</div><label><input type="checkbox" checked={installedOnly} onChange={e => setInstalledOnly(e.target.checked)} />{marketText(lang, '只看已安装', 'Installed only')}</label></div>
        {snapshot.preview && <p className="market-muted">{marketText(lang, '浏览器预览 · 安装和使用应用请打开 dsivio 桌面端。', 'Browser preview · Install and use apps in the dsivio desktop app.')}</p>}
        {snapshot.error && <div role="status" className="market-notice">{snapshot.refreshedAt ? marketText(lang, '暂时无法刷新，已显示上次内容。', 'Refresh failed. Showing cached content.') : marketText(lang, '暂时无法读取市场。', 'The market could not be loaded.')}<details><summary>{marketText(lang, '查看原因', 'Details')}</summary>{snapshot.error}</details><Button size="sm" onClick={() => void marketApi.refresh()}>{marketText(lang, '重试', 'Retry')}</Button></div>}
        {initialLoading && !items.length ? <div className="market-grid" aria-busy="true">{[0, 1, 2].map(i => <div className="market-skeleton" key={i} />)}</div> : !visible.length ? <div className="market-empty"><LayoutGrid size={28} /><h2>{marketText(lang, query ? '没有找到相关应用' : installedOnly ? '还没有安装应用' : snapshot.error ? '市场暂不可用' : '暂无应用', query ? 'No matching apps' : installedOnly ? 'No apps installed yet' : snapshot.error ? 'Market unavailable' : 'No apps yet')}</h2><p>{marketText(lang, query || category || installedOnly ? '试试其他分类，或查看全部应用。' : '团队发布后会显示在这里，无需更新软件。', 'Published apps will appear here without an app update.')}</p>{(query || category || installedOnly) && <Button size="sm" onClick={() => { setQuery(''); setCategory(''); setInstalledOnly(false) }}>{marketText(lang, '查看全部', 'Show all')}</Button>}</div> : <div className="market-grid">{visible.map(item => <article key={item.id} className="market-card"><button className="market-card-info" onClick={() => { window.location.hash = `#chat/market/${encodeURIComponent(item.id)}` }}><div><PackageIcon item={item} categories={item.manifest?.categoryIds} /><h2>{item.manifest?.name ?? item.id}</h2></div><p>{item.manifest?.summary ?? item.entry?.error}</p></button><div className="market-card-meta">{!item.entry && <span>{marketText(lang, '已下架', 'Unlisted')}</span>}{item.entry && item.local?.status === 'ready' && item.entry.version !== item.local.manifest.version && <span>{marketText(lang, '市场有新版', 'New version listed')}</span>}{item.local?.status !== 'ready' && <span>{marketText(lang, item.local ? '安装未完成' : item.entry?.error ? '暂不可用' : '未安装', item.local ? 'Setup incomplete' : item.entry?.error ? 'Unavailable' : 'Not installed')}</span>}</div><footer><LoadSwitch item={item} busy={busyIds.has(item.id)} onChange={() => toggle(item)} lang={lang} />{more(item)}<span className="market-primary">{primary(item)}</span></footer></article>)}</div>}
      </>}
      {error && <p className="market-error" role="alert">{error}</p>}
    </div>
    {selected && chosen?.manifest && <footer className="market-detail-footer"><LoadSwitch item={chosen} busy={busyIds.has(chosen.id)} onChange={() => toggle(chosen)} lang={lang} /><span className="market-primary">{primary(chosen)}</span></footer>}

  </section>
}
