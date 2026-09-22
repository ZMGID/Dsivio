import { useCallback, useEffect, useRef, useState } from 'react'
import { BookmarkPlus, Check, Clock, ImagePlus, ScanSearch, Settings2, X } from 'lucide-react'
import { api } from '../../../api/tauri'
import { getSettingsCached, subscribeSettings, updateSettingsCached } from '../../../api/settingsCache'
import { Button, IconButton } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import type { ProductSort, SourcingProduct } from '../../../generated/sourcing'
import { Input, Select } from '../../../settings/public/controls'
import { WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { TemplateDialog } from '../content/TemplateDialog'
import '../content/templateLibrary.css'
import { dataUrl } from '../localMedia'
import { ProductCard } from './ProductCard'
import { productDraft } from './productDraft'
import { useSourcingSearch } from './useSourcingSearch'

export function LookalikePage() {
  const t = useT(), en = useLang() === 'en'
  const search = useSourcingSearch()
  const { clear: clearSearch, pending: searchPending } = search
  const fileRef = useRef<HTMLInputElement>(null)
  const alive = useRef(true), fileVersion = useRef(0), saving = useRef(false)
  const [file, setFile] = useState<{ name: string; data: string } | null>(null)
  const [reading, setReading] = useState(false)
  const [dragging, setDragging] = useState(false)
  const dragDepth = useRef(0)
  const [sort, setSort] = useState<ProductSort>(() => {
    const value = new URLSearchParams(window.location.search).get('matchSort')
    return value === 'priceAsc' || value === 'priceDesc' || value === 'salesDesc' ? value : 'relevance'
  })
  const [quantity, setQuantity] = useState('1')
  const [query, setQuery] = useState(() => new URLSearchParams(window.location.search).get('matchQuery') ?? '')
  const [dialog, setDialog] = useState<'history' | 'config' | null>(null)
  const [ak, setAk] = useState('')
  const [savedAk, setSavedAk] = useState('')
  const configured = !!savedAk
  const [configError, setConfigError] = useState('')
  const [configPending, setConfigPending] = useState(false)
  const settingsRead = useRef(0)
  const [configLoaded, setConfigLoaded] = useState(false)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [saved, setSaved] = useState<Set<string>>(new Set())
  const loadConfig = useCallback(async () => {
    const version = ++settingsRead.current
    setConfigPending(true); setConfigError('')
    try {
      const settings = await getSettingsCached()
      if (alive.current && version === settingsRead.current) {
        setSavedAk(settings.sourcing?.alibabaAk || ''); setConfigLoaded(true)
      }
    } catch (e) { if (alive.current && version === settingsRead.current) setConfigError(String(e)) }
    finally { if (alive.current && version === settingsRead.current) setConfigPending(false) }
  }, [])
  useEffect(() => {
    alive.current = true
    const unsubscribe = subscribeSettings(settings => {
      settingsRead.current++
      setSavedAk(settings.sourcing?.alibabaAk || ''); setConfigLoaded(true); setConfigError(''); setConfigPending(false)
    })
    void loadConfig()
    return () => { unsubscribe(); alive.current = false }
  }, [loadConfig])
  const pickFile = useCallback(async (value?: File) => {
    if (!value || searchPending) return
    const version = ++fileVersion.current
    setError(''); setNotice(''); setReading(false)
    if (!['image/png', 'image/jpeg', 'image/webp'].includes(value.type)) { setError(en ? 'Use PNG, JPEG or WebP.' : '请选择 PNG、JPEG 或 WebP 图片'); return }
    if (value.size > 10 * 1024 * 1024) { setError(t.workbenchMatchTooBig); return }
    setReading(true)
    try {
      const data = await dataUrl(value)
      if (alive.current && version === fileVersion.current) { setFile({ name: value.name, data }); clearSearch(); setSaved(new Set()) }
    } catch (e) { if (alive.current && version === fileVersion.current) setError(String(e)) }
    finally { if (alive.current && version === fileVersion.current) setReading(false) }
  }, [searchPending, clearSearch, en, t.workbenchMatchTooBig]) // clear only changes local search state
  useEffect(() => {
    const paste = (event: ClipboardEvent) => {
      if (dialog) return
      const image = [...event.clipboardData?.items ?? []].find(i => i.type.startsWith('image/'))?.getAsFile()
      if (image) { event.preventDefault(); void pickFile(image) }
    }
    window.addEventListener('paste', paste)
    return () => window.removeEventListener('paste', paste)
  }, [pickFile, dialog])
  const perform = async (action: () => Promise<void>) => {
    if (saving.current) return
    saving.current = true; setBusy(true); setError(''); setNotice('')
    try { await action() } catch (e) { if (alive.current) setError(String(e)) }
    finally { saving.current = false; if (alive.current) setBusy(false) }
  }
  const collect = (product: SourcingProduct) => perform(async () => {
    const draft = productDraft(product)
    await api.sourcingSavePick({ id: null, expectedRevision: null, product: draft })
    if (alive.current) { setSaved(s => new Set([...s, draft.sourceId!])); setNotice(en ? 'Saved to pick library.' : '已加入选品库') }
  })
  const products = search.result?.products.filter(p => `${p.title} ${p.supplier ?? ''} ${p.skuTitle ?? ''}`.toLowerCase().includes(query.trim().toLowerCase())) ?? []
  useEffect(() => {
    const url = new URL(window.location.href)
    for (const [key, value] of [['matchSort', sort === 'relevance' ? '' : sort], ['matchQuery', query]]) { if (value) url.searchParams.set(key, value); else url.searchParams.delete(key) }
    window.history.replaceState(window.history.state, '', url)
  }, [query, sort])
  const close = () => { if (!busy && !search.pending) setDialog(null) }
  return <div className="sourcing-match"><WorkbenchPage crumb={t.workbenchGroupSourcing} title={t.workbenchNavMatch}
    subtitle={en ? 'Upload a product photo to find matching products on 1688.' : '上传商品图，在 1688 查找同款及相似款货源。'}
    actions={<><Button variant="ghost" disabled={!configLoaded || search.pending} onClick={() => { setError(''); setAk(savedAk); setDialog('config') }}><Settings2 size={14} />{en ? '1688 settings' : '1688 接口设置'}</Button><Button variant="ghost" disabled={search.pending || reading} onClick={() => { setError(''); setDialog('history'); void search.refreshHistory() }}><Clock size={14} />{t.workbenchMatchHistory}</Button></>}>
    <section className="match-search-panel" aria-label={en ? 'Image search' : '图片搜索'}>
      <div className={`match-upload${dragging ? ' is-dragging' : ''}`} onDragEnter={event => {
        event.preventDefault(); if (!search.pending) { dragDepth.current++; setDragging(true) }
      }} onDragOver={event => event.preventDefault()} onDragLeave={event => {
        event.preventDefault(); if (--dragDepth.current <= 0) { dragDepth.current = 0; setDragging(false) }
      }} onDrop={event => {
        event.preventDefault(); dragDepth.current = 0; setDragging(false); void pickFile(event.dataTransfer.files[0])
      }}>
        {file ? <>
          <img className="match-upload-preview" src={file.data} alt={file.name} />
          <div className="match-upload-remove"><IconButton label={en ? 'Remove image' : '移除图片'} size="sm" disabled={search.pending || reading} onClick={() => {
            fileVersion.current++; setFile(null); clearSearch(); setSaved(new Set()); setError(''); setNotice('')
          }}><X size={14} /></IconButton></div>
          <div className="match-upload-replace"><Button size="sm" disabled={search.pending || reading} onClick={() => fileRef.current?.click()}>{en ? 'Replace image' : '更换图片'}</Button></div>
        </> : <>
          <ImagePlus size={28} strokeWidth={1.5} aria-hidden />
          <Button disabled={search.pending || reading} onClick={() => fileRef.current?.click()}>{reading ? (en ? 'Reading…' : '正在读取…') : t.workbenchMatchUpload}</Button>
          <span>{dragging ? (en ? 'Drop to upload' : '松开即可上传') : (en ? 'or drop / paste an image' : '或将图片拖入 / 粘贴到此处')}</span>
        </>}
        <input ref={fileRef} type="file" aria-label={t.workbenchMatchUpload} accept="image/png,image/jpeg,image/webp" hidden onChange={event => { void pickFile(event.target.files?.[0]); event.target.value = '' }} />
      </div>
      <div className="match-search-options">
        <div className="match-panel-heading"><h2>{en ? 'Search settings' : '搜索条件'}</h2><span className="match-platform">1688 {en ? 'products' : '货源'}</span></div>
        <p className="match-image-hint" title={file?.name}>{file ? file.name : (en ? 'Use a clear photo showing a single product.' : '建议使用主体清晰、背景简洁的单件商品图')}</p>
        <form className="match-search-controls" onSubmit={event => {
          event.preventDefault(); setError(''); setNotice('')
          if (!file) { setError(t.workbenchMatchNeedImage); return }
          if (!configured) { setAk(savedAk); setDialog('config'); return }
          if (!/^\d+$/.test(quantity) || Number(quantity) < 1 || Number(quantity) > 1000000) { setError(en ? 'Quantity must be 1–1000000.' : '采购件数必须为 1–1000000 的整数'); return }
          if (!reading) { setSaved(new Set()); void search.search({ image: file.data, name: file.name, sort, limit: 30, purchaseAmount: Number(quantity) }) }
        }}>
          <label className="sourcing-field match-sort">{en ? 'Sort' : '搜索排序'}<Select value={sort} disabled={search.pending} onChange={v => setSort(v as ProductSort)} ariaLabel={en ? 'Sort' : '搜索排序'} options={[{ value: 'relevance', label: en ? 'Relevance' : '相关性优先' }, { value: 'priceAsc', label: en ? 'Price: low to high' : '价格从低到高' }, { value: 'priceDesc', label: en ? 'Price: high to low' : '价格从高到低' }, { value: 'salesDesc', label: en ? 'Sales' : '销量优先' }]} /></label>
          <label className="sourcing-field match-quantity">{en ? 'Quantity' : '采购件数'}<Input value={quantity} onChange={setQuantity} disabled={search.pending} inputMode="numeric" /></label>
          <div className="match-submit"><Button type="submit" variant="primary" disabled={search.pending || reading || !configLoaded} aria-busy={search.pending}><ScanSearch size={16} />{search.pending ? (en ? 'Searching…' : '正在找同款…') : (en ? 'Search 1688' : '在 1688 找同款')}</Button></div>
        </form>
        <p className="match-format-hint">PNG / JPEG / WebP · {en ? 'Up to 10MB' : '最大 10MB'}<span>{en ? 'Images are sent to 1688 when you search.' : '搜索时将图片发送至 1688'}</span></p>
      </div>
    </section>
    {configError && <div><p className="sourcing-error" role="alert">{configError}</p><Button disabled={configPending} onClick={() => void loadConfig()}>{configPending ? (en ? 'Loading…' : '正在读取…') : (en ? 'Retry settings' : '重试读取设置')}</Button></div>}
    {!configured && configLoaded && <p className="sourcing-notice">{en ? 'Configure your 1688 AK before searching.' : '首次使用请在「1688 接口设置」中填写 AK。'}</p>}
    {(error || search.error) && !dialog && <p className="sourcing-error" role="alert">{error || search.error}</p>}
    {search.result?.historyWarning && <p className="sourcing-error" role="alert">{search.result.historyWarning}</p>}
    {notice && <p className="sourcing-notice" role="status">{notice}</p>}
    <section className="match-results" aria-label={t.workbenchMatchResult}>
      <div className="match-results-heading">
        <div className="match-results-title"><h2>{t.workbenchMatchResult}</h2>{search.result && <span className="match-result-count">{en ? `${products.length} products` : `${products.length} 件商品`}</span>}</div>
        {search.result && <label className="match-results-filter"><span className="sr-only">{t.workbenchMatchFilter}</span><Input type="search" placeholder={en ? 'Filter title or supplier' : '筛选商品名称、供应商'} value={query} onChange={setQuery} /></label>}
      </div>
      {search.result && <p className="match-result-meta">{search.result.name} · {new Date(search.result.fetchedAt).toLocaleString()} · {en ? 'Compare specifications; quotes may change.' : '请核对商品规格，报价以商品页面为准'}</p>}
      {search.pending ? <div className="match-empty" role="status"><ScanSearch size={26} strokeWidth={1.5} /><div><h3>{en ? 'Searching 1688…' : '正在匹配 1688 货源…'}</h3><p>{en ? 'Finding products and supplier quotes for your image.' : '正在根据商品图查找同款及相似款货源'}</p></div></div> : products.length ? <div className="sourcing-grid">{products.map(p => <ProductCard key={`${search.result?.id}:${p.id}:${p.skuId}`} product={productDraft(p)} details={<>
        {p.skuTitle && <p>{p.skuTitle}</p>}
        <div className="match-product-facts">{p.minimumOrder && <span>{en ? 'Min. order' : '起批'} {p.minimumOrder}</span>}{p.soldCount && <span>{en ? 'Sold' : '已售'} {p.soldCount}</span>}</div>
      </>}><Button size="sm" disabled={busy || saved.has(productDraft(p).sourceId!)} onClick={() => void collect(p)}>{saved.has(productDraft(p).sourceId!) ? <Check size={14} /> : <BookmarkPlus size={14} />}{saved.has(productDraft(p).sourceId!) ? (en ? 'Saved' : '已收藏') : (en ? 'Save to library' : '加入选品库')}</Button></ProductCard>)}</div> : <div className="match-empty"><ScanSearch size={28} strokeWidth={1.5} /><div><h3>{search.result ? (en ? 'No matching products' : '没有找到匹配商品') : (en ? 'Your matches will appear here' : '上传商品图，开始寻找同款')}</h3><p>{search.result ? (en ? 'Try another product photo or adjust the filter.' : '换一张清晰的商品图，或调整筛选条件试试') : (en ? 'Compare prices, minimum orders and suppliers, then save your picks.' : '对比价格、起批量和供应商，将合适的货源加入选品库')}</p></div></div>}
    </section>
    {dialog === 'config' && <TemplateDialog title={en ? '1688 settings' : '1688 接口设置'} closeLabel={en ? 'Close' : '关闭'} onClose={close} busy={busy} footer={<Button variant="primary" disabled={busy} onClick={() => void perform(async () => {
      await updateSettingsCached(s => ({ ...s, sourcing: { alibabaAk: ak.trim() } }))
      if (alive.current) { setSavedAk(ak.trim()); setDialog(null); setNotice(en ? 'Settings saved.' : '接口设置已保存') }
    })}>{busy ? (en ? 'Saving…' : '保存中…') : (en ? 'Save' : '保存')}</Button>}>
      <div className="sourcing-form"><label className="sourcing-field">1688 AK<Input value={ak} onChange={setAk} type="password" autoComplete="off" disabled={busy} /></label>
      <p className="sourcing-notice">{en ? 'Use the AK issued for 1688-product-find. Saved locally with application settings; never included in search history.' : '填写 1688-product-find 对应的完整 AK，随应用设置保存在本机，不写入搜索历史。'}</p>
      <Button disabled={busy} onClick={() => void perform(() => api.openExternal('https://clawhub.1688.com/'))}>{en ? 'Open 1688 authorization site' : '打开 1688 授权网站'}</Button>
      {error && <p className="sourcing-error" role="alert">{error}</p>}</div>
    </TemplateDialog>}
    {dialog === 'history' && <TemplateDialog title={t.workbenchMatchHistory} closeLabel={en ? 'Close' : '关闭'} onClose={close} busy={busy || search.pending}>
      <p className="sourcing-notice">{en ? 'Reopen saved results without another search. Keeps the latest 100 searches.' : '查看已保存的结果，不重复调用接口；保留最近 100 次搜索。'}</p>
      {(error || search.error || search.historyError) && <p className="sourcing-error" role="alert">{error || search.error || search.historyError}</p>}
      <Button size="sm" disabled={search.historyPending} onClick={() => void search.refreshHistory()}>{search.historyPending ? (en ? 'Loading…' : '正在读取…') : (en ? 'Refresh history' : '刷新记录')}</Button>
      <ul className="sourcing-history">{search.history.map(h => <li key={h.id}><span>{h.name}<br />{new Date(h.fetchedAt).toLocaleString()} · {h.count}</span><Button size="sm" disabled={busy || search.pending} onClick={() => {
        void search.restore(h.id).then(ok => { if (ok && alive.current) { fileVersion.current++; setReading(false); setFile(null); setQuery(''); setSaved(new Set()); setDialog(null) } })
      }}>{en ? 'View results' : '查看结果'}</Button></li>)}</ul>
      {!search.historyPending && !search.historyError && !search.history.length && <WorkbenchEmpty>{t.workbenchMatchHistoryEmpty}</WorkbenchEmpty>}
    </TemplateDialog>}
  </WorkbenchPage></div>
}
