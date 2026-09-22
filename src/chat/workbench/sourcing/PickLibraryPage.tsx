import { useEffect, useRef, useState } from 'react'
import { Package } from 'lucide-react'
import { api } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import type { PickDraft, PickItem, PickPage, PickStage } from '../../../generated/sourcing'
import { Input, Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { TemplateDialog } from '../content/TemplateDialog'
import '../content/templateLibrary.css'
import { ProductCard } from './ProductCard'

const blank = (): PickDraft => ({ source: 'manual', sourceId: null, title: '', url: null, imageUrl: null, price: null, currency: 'CNY', supplier: null, tags: [], note: '', stage: 'new' })
function stageOptions(en: boolean) {
  return [{ value: 'new', label: en ? 'New' : '待评估' }, { value: 'evaluating', label: en ? 'Evaluating' : '评估中' }, { value: 'selected', label: en ? 'Selected' : '已选中' }, { value: 'rejected', label: en ? 'Rejected' : '已淘汰' }]
}
function PickEditor({ item, en, onClose, onSaved }: { item: PickItem | null; en: boolean; onClose: () => void; onSaved: () => void }) {
  const [draft, setDraft] = useState<PickDraft>(() => item?.product ?? blank())
  const [tags, setTags] = useState(item?.product.tags.join(', ') ?? '')
  const [busy, setBusy] = useState(false), [error, setError] = useState(''), [discard, setDiscard] = useState(false)
  const saving = useRef(false), alive = useRef(true)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  const dirty = JSON.stringify(draft) !== JSON.stringify(item?.product ?? blank()) || tags !== (item?.product.tags.join(', ') ?? '')
  const close = () => { if (!saving.current) { if (dirty) setDiscard(true); else onClose() } }
  const update = <K extends keyof PickDraft>(key: K, value: PickDraft[K]) => setDraft(d => ({ ...d, [key]: value }))
  const save = async () => {
    if (saving.current) return
    saving.current = true; setBusy(true); setError('')
    try {
      await api.sourcingSavePick({ id: item?.id ?? null, expectedRevision: item?.revision ?? null, product: { ...draft, tags: tags.split(/[,，]/).map(x => x.trim()).filter(Boolean) } })
      if (alive.current) onSaved()
    } catch (e) { if (alive.current) setError(String(e)) }
    finally { saving.current = false; if (alive.current) setBusy(false) }
  }
  return <TemplateDialog title={discard ? (en ? 'Discard unsaved changes?' : '放弃未保存的修改？') : (item ? (en ? 'Edit product' : '编辑选品') : (en ? 'Add product' : '添加选品'))} closeLabel={en ? 'Close' : '关闭'} onClose={discard ? () => setDiscard(false) : close} busy={busy} footer={discard ? <><Button onClick={() => setDiscard(false)}>{en ? 'Keep editing' : '继续编辑'}</Button><Button variant="danger" onClick={onClose}>{en ? 'Discard' : '放弃修改'}</Button></> : <><Button disabled={busy} onClick={close}>{en ? 'Cancel' : '取消'}</Button><Button variant="primary" disabled={busy} type="submit" form="sourcing-pick-editor">{busy ? (en ? 'Saving…' : '保存中…') : (en ? 'Save' : '保存')}</Button></>}>
    {discard ? <p>{en ? 'Changes have not been saved.' : '关闭后，本次修改不会保存。'}</p> : <form id="sourcing-pick-editor" className="sourcing-form" onSubmit={event => { event.preventDefault(); void save() }}>
      <fieldset disabled={busy} className="sourcing-editor-fields">
        <label className="sourcing-field">{en ? 'Title' : '商品名称'}<Input value={draft.title} onChange={v => update('title', v)} required maxLength={250} /></label>
        <label className="sourcing-field">{en ? 'Product link' : '商品链接'}<Input value={draft.url ?? ''} onChange={v => update('url', v)} type="url" /></label>
        <label className="sourcing-field">{en ? 'Image URL' : '商品图片链接'}<Input value={draft.imageUrl ?? ''} onChange={v => update('imageUrl', v)} type="url" /></label>
        <div className="sourcing-form-row"><label className="sourcing-field">{en ? 'Price' : '报价'}<Input value={draft.price ?? ''} onChange={v => update('price', v)} /></label><label className="sourcing-field">{en ? 'Currency' : '币种'}<Input value={draft.currency ?? ''} onChange={v => update('currency', v)} /></label></div>
        <label className="sourcing-field">{en ? 'Supplier' : '供应商 / 店铺'}<Input value={draft.supplier ?? ''} onChange={v => update('supplier', v)} /></label>
        <label className="sourcing-field">{en ? 'Tags (comma separated)' : '标签（逗号分隔）'}<Input value={tags} onChange={setTags} /></label>
        <div className="sourcing-field"><span>{en ? 'Status' : '选品状态'}</span><Select value={draft.stage} disabled={busy} onChange={v => update('stage', v as PickStage)} options={stageOptions(en)} ariaLabel={en ? 'Status' : '选品状态'} /></div>
        <label className="sourcing-field">{en ? 'Notes / specifications' : '备注 / 规格'}<TextArea value={draft.note} onChange={v => update('note', v)} rows={4} /></label>
      </fieldset>
      {error && <p className="sourcing-error" role="alert">{error}</p>}
    </form>}
  </TemplateDialog>
}

export function PickLibraryPage() {
  const t = useT(), en = useLang() === 'en'
  const [query, setQuery] = useState(() => new URLSearchParams(window.location.search).get('picksQuery') ?? '')
  const [stage, setStage] = useState(() => {
    const value = new URLSearchParams(window.location.search).get('picksStage') ?? ''
    return stageOptions(false).some(x => x.value === value) ? value : ''
  })
  const [page, setPage] = useState(() => {
    const value = Number(new URLSearchParams(window.location.search).get('picksPage'))
    return Number.isInteger(value) && value >= 1 && value <= 100000 ? value : 1
  }), [revision, setRevision] = useState(0)
  const [data, setData] = useState<PickPage>({ items: [], total: 0 })
  const [pending, setPending] = useState(true), [error, setError] = useState(''), [notice, setNotice] = useState('')
  const [editor, setEditor] = useState<{ item: PickItem | null } | null>(null)
  const [deleting, setDeleting] = useState<PickItem | null>(null), [deletingBusy, setDeletingBusy] = useState(false), [deleteError, setDeleteError] = useState('')
  const alive = useRef(true), deleteRunning = useRef(false)
  useEffect(() => { alive.current = true; return () => { alive.current = false } }, [])
  useEffect(() => {
    const url = new URL(window.location.href)
    for (const [key, value] of [['picksQuery', query], ['picksStage', stage], ['picksPage', page === 1 ? '' : String(page)]]) { if (value) url.searchParams.set(key, value); else url.searchParams.delete(key) }
    window.history.replaceState(window.history.state, '', url)
    let active = true
    setPending(true); setError('')
    const timer = window.setTimeout(() => {
      void api.sourcingListPicks({ keyword: query, stage: stage ? stage as PickStage : null, source: null, page, pageSize: 24 }).then(value => {
        if (active) {
          if (!value.items.length && page > Math.max(1, Math.ceil(value.total / 24))) setPage(Math.max(1, Math.ceil(value.total / 24)))
          else setData(value)
        }
      }).catch(e => { if (active) setError(String(e)) }).finally(() => { if (active) setPending(false) })
    }, 200)
    return () => { active = false; clearTimeout(timer) }
  }, [query, stage, page, revision])
  const remove = async () => {
    if (!deleting || deleteRunning.current) return
    deleteRunning.current = true; setDeletingBusy(true); setDeleteError('')
    try {
      await api.sourcingDeletePick(deleting.id, deleting.revision)
      if (alive.current) { setDeleting(null); setRevision(r => r + 1); setNotice(en ? 'Product deleted.' : '选品已删除') }
    } catch (e) { if (alive.current) setDeleteError(String(e)) }
    finally { deleteRunning.current = false; if (alive.current) setDeletingBusy(false) }
  }
  return <WorkbenchPage crumb={t.workbenchGroupSourcing} title={t.workbenchNavPicks} subtitle={en ? 'Keep products, source links and selection notes on this device.' : '保存同款货源、商品链接和选品记录，重启后可继续查看。'} actions={<Button size="sm" onClick={() => setEditor({ item: null })}>{en ? 'Add product' : '添加选品'}</Button>}>
    <div className="sourcing-toolbar">
      <label className="sourcing-field">{en ? 'Search title, tags or notes' : '搜索名称、标签或备注'}<Input type="search" value={query} onChange={v => { setQuery(v); setPage(1) }} /></label>
      <div className="sourcing-field"><span>{en ? 'Status' : '选品状态'}</span><Select value={stage} onChange={v => { setStage(v); setPage(1) }} ariaLabel={en ? 'Filter by status' : '筛选选品状态'} options={[{ value: '', label: en ? 'All' : '全部状态' }, ...stageOptions(en)]} /></div>
      <Button disabled={pending} onClick={() => setRevision(r => r + 1)}>{en ? 'Refresh' : '刷新'}</Button>
    </div>
    <div className="workbench-page-meta"><span>{t.workbenchPicksCount.replace('{n}', pending || error ? '—' : String(data.total))}</span></div>
    {error && <p className="sourcing-error" role="alert">{error}</p>}
    {notice && <p className="sourcing-notice" role="status">{notice}</p>}
    <WorkbenchCard fill>
      {pending ? <WorkbenchEmpty>{en ? 'Loading…' : '正在加载…'}</WorkbenchEmpty> : error ? <WorkbenchEmpty>{en ? 'Could not load products. Use Refresh to retry.' : '选品加载失败，请点击「刷新」重试。'}</WorkbenchEmpty> : data.items.length ? <div className="sourcing-grid">{data.items.map(item => <ProductCard key={`${item.id}:${item.revision}`} product={item.product} details={<>
        <p>{item.product.source === 'alibaba1688' ? '1688' : (en ? 'Manual' : '手动添加')} · {stageOptions(en).find(x => x.value === item.product.stage)?.label}</p>
        {!!item.product.tags.length && <p>{item.product.tags.join(' · ')}</p>}
        {item.product.note && <p>{item.product.note}</p>}
      </>}><Button size="sm" onClick={() => setEditor({ item })}>{en ? 'Edit' : '编辑'}</Button><Button size="sm" variant="danger" onClick={() => { setDeleteError(''); setDeleting(item) }}>{en ? 'Delete' : '删除'}</Button></ProductCard>)}</div> : <WorkbenchEmpty icon={<Package size={22} />} title={query || stage ? (en ? 'No matching products' : '没有符合筛选条件的选品') : t.workbenchPicksEmpty}>{en ? 'Save a match from 1688 or add a product manually.' : '从同款找货收藏商品，或点击「添加选品」手动录入。'}</WorkbenchEmpty>}
      <div className="sourcing-pagination"><Button size="sm" disabled={pending || !!error || page === 1} onClick={() => setPage(p => p - 1)}>{en ? 'Previous' : '上一页'}</Button><span>{page} / {Math.max(1, Math.ceil(data.total / 24))}</span><Button size="sm" disabled={pending || !!error || page * 24 >= data.total} onClick={() => setPage(p => p + 1)}>{en ? 'Next' : '下一页'}</Button></div>
    </WorkbenchCard>
    {editor && <PickEditor item={editor.item} en={en} onClose={() => setEditor(null)} onSaved={() => { setEditor(null); setRevision(r => r + 1); setNotice(en ? 'Product saved.' : '选品已保存') }} />}
    {deleting && <TemplateDialog title={en ? 'Delete this product?' : '删除这条选品？'} closeLabel={en ? 'Close' : '关闭'} busy={deletingBusy} onClose={() => { if (!deletingBusy) setDeleting(null) }} footer={<><Button disabled={deletingBusy} onClick={() => setDeleting(null)}>{en ? 'Cancel' : '取消'}</Button><Button variant="danger" disabled={deletingBusy} onClick={() => void remove()}>{deletingBusy ? (en ? 'Deleting…' : '删除中…') : (en ? 'Delete product' : '确认删除')}</Button></>}><p className="sourcing-notice">{deleting.product.title}</p><p>{en ? 'This removes the local record and notes. It cannot be undone.' : '将删除本机保存的选品记录和备注，无法撤销。'}</p>{deleteError && <p className="sourcing-error" role="alert">{deleteError}</p>}</TemplateDialog>}
  </WorkbenchPage>
}
