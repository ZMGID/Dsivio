import { useEffect, useState, type ReactNode } from 'react'
import { Layers3 } from 'lucide-react'
import { TemplateDialog } from './TemplateDialog'
import './templateLibrary.css'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import type { ImageTemplate, ImageTemplateSlot } from '../../../api/workbenchImageContracts'
import { Button } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import { Input, TextArea } from '../../../settings/public/controls'
import { WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { useTemplateLibrary } from './useTemplateLibrary'

function Example({ id, reference, name = reference, fallback }: { id: string; reference: string; name?: string; fallback?: ReactNode }) {
  const [url, setUrl] = useState('')
  const [error, setError] = useState('')
  useEffect(() => {
    let alive = true
    setUrl(''); setError('')
    void api.imageTemplatePreview(id, reference).then(value => { if (alive) setUrl(value) })
      .catch(e => { if (alive) setError(String(e)) })
    return () => { alive = false }
  }, [id, reference])
  if (url) return <img src={url} alt={name} />
  return <>{fallback || <p role={error ? 'alert' : 'status'}>{error || '…'}</p>}</>
}

const slotDescription = (slot: ImageTemplateSlot) => slot.brief?.trim() || slot.prompt?.trim() || ''

function TemplatePagePreview({ template, initialIndex, onClose, en }: {
  template: ImageTemplate; initialIndex: number; onClose: () => void; en: boolean
}) {
  const [index, setIndex] = useState(initialIndex)
  const slot = template.data.slots[index]
  return <TemplateDialog className="content-template-preview-dialog" title={`${template.data.name} · ${en ? 'Page preview' : '页面预览'}`} closeLabel={en ? 'Close preview' : '关闭页面预览'} onClose={onClose} footer={<>
    <Button disabled={index === 0} onClick={() => setIndex(i => i - 1)}>{en ? 'Previous' : '上一张'}</Button>
    <Button disabled={index === template.data.slots.length - 1} onClick={() => setIndex(i => i + 1)}>{en ? 'Next' : '下一张'}</Button>
  </>}>
    <p>{index + 1} / {template.data.slots.length} · {slot.id}</p>
    <h3>{slot.purpose || `${en ? 'Page' : '页面'} ${index + 1}`}</h3>
    {slot.example && <div className="content-template-preview-image"><Example id={template.id} reference={slot.example} name={`${template.data.name} · ${slot.purpose || slot.id}`} /></div>}
    <p className="content-template-preview-description">{slotDescription(slot) || (en ? 'No page description yet.' : '此页尚未填写画面说明，可在编辑中补充。')}</p>
  </TemplateDialog>
}

export function ImageTemplatesPage() {
  const t = useT()
  const en = useLang() === 'en'
  const library = useTemplateLibrary(api.imageTemplatesList)
  const [preview, setPreview] = useState<{ template: ImageTemplate; index: number } | null>(null)
  const { editing, setEditing, pending, perform, accept } = library
  const words = en ? { refresh: 'Refresh', import: 'Import folder', export: 'Export', name: 'Name', category: 'Category', language: 'Language', style: 'Shared style', ratio: 'Aspect ratio', resolution: 'Resolution', purpose: 'Purpose', rule: 'Page rules', saved: 'Template saved', copy: 'Save as user template', empty: 'No templates', edit: 'View / edit', create: 'Create rule template', add: 'Add page', remove: 'Remove page' } : { refresh: '刷新', import: '导入文件夹', export: '导出', name: '模板名称', category: '商品品类', language: '图内语言', style: '统一风格', ratio: '比例', resolution: '分辨率', purpose: '页面用途', rule: '页面规则', saved: '模板已保存', copy: '另存为用户模板', empty: '暂无模板', edit: '查看 / 编辑', create: '创建规则模板', add: '添加页面', remove: '移除此页' }
  const patch = (data: Partial<ImageTemplate['data']>) => setEditing(old => old && ({ ...old, data: { ...old.data, ...data } }))
  const patchSlot = (index: number, data: Partial<ImageTemplateSlot>) => editing && patch({ slots: editing.data.slots.map((s, i) => i === index ? { ...s, ...data } : s) })
  const field = (name: string, value: string, change: (value: string) => void, multiline = false) => <label className="flex min-w-0 flex-col gap-2"><span>{name}</span>{multiline ? <TextArea value={value} onChange={change} rows={4} /> : <Input value={value} onChange={change} />}</label>
  return <WorkbenchPage crumb={t.workbenchGroupContent} title={t.workbenchNavImageTemplates} actions={<>
    <Button disabled={pending} onClick={() => void library.refresh()}>{words.refresh}</Button>
    <Button disabled={pending} onClick={() => void perform(async () => { const path = await open({ directory: true }); if (typeof path === 'string') accept(await api.imageTemplateImport(path)) })}>{words.import}</Button>
    <Button disabled={pending} onClick={() => setEditing({ id: '', directory: '', builtin: false, data: { name: words.create, mode: 'smart', slots: [{ id: 'h1', purpose: '', brief: '' }] } })}>{words.create}</Button>
  </>}>
    {!editing && library.error && <p role="alert" className="workbench-inline-note">{library.error}</p>}
    {!editing && library.notice && <p role="status">{library.notice}</p>}
    {pending && <p role="status">{en ? 'Working…' : '处理中…'}</p>}
    <div className="content-image-templates">
      {!pending && !library.items.length && <WorkbenchEmpty title={words.empty} />}
      <div className="ct-image-grid">
        {library.items.map((t) => (
          <article className="ct-image-card" key={t.id}>
            <div className={`ct-image-cover${t.data.slots.slice(0, 3).some((slot) => slot.example && t.directory) ? '' : ' ct-image-outline'}`}>
              <div className="ct-image-cover-heading">
                <span>{t.data.mode === 'replace' ? (en ? 'Sample replacement' : '样图换货') : (en ? 'Style rules' : '风格规则')}</span>
                <span>{t.data.slots.length} {en ? 'pages' : '张'}</span>
              </div>
              {!t.data.slots.slice(0, 3).some((slot) => slot.example && t.directory) && <div className="ct-image-outline-heading"><Layers3 size={20} strokeWidth={1.5} /><span>{en ? 'Page outline' : '页面安排'}</span></div>}
              <div className="ct-image-pages">
                {t.data.slots.slice(0, 3).map((slot, index) => {
                  const purpose = slot.purpose || (en ? `Page ${index + 1}` : `第 ${index + 1} 张`)
                  const summary = (
                    <span className="ct-image-slot-summary">
                      <small>{en ? 'Summary' : '内容概要'}</small>
                      <span>{slotDescription(slot)}</span>
                    </span>
                  )
                  // Content-navigation tile; Button.tsx excludes this interaction from CTA primitives.
                  return (
                    <button
                      type="button" disabled={pending}
                      key={slot.id}
                      className="ct-image-slot"
                      aria-label={en ? `${t.data.name} · Page ${index + 1}: ${purpose}` : `${t.data.name} · 第 ${index + 1} 张：${purpose}`}
                      onClick={() => setPreview({ template: t, index })}
                    >
                      <span className="ct-image-slot-visual">
                        {slot.example && t.directory ? (
                          <Example
                            id={t.id} reference={slot.example}
                            name={en ? `${purpose} reference` : `${purpose}参考图`}
                            fallback={summary}
                          />
                        ) : (
                          summary
                        )}
                      </span>
                      <span className="ct-image-slot-caption">
                        <span>{String(index + 1).padStart(2, '0')}</span>
                        <strong>{purpose}</strong>
                      </span>
                    </button>
                  )
                })}
              </div>
            </div>
            <div className="ct-image-info">
              <h3>
                {t.data.name}
                {t.builtin && <small>{en ? 'Built-in' : '内置'}</small>}
              </h3>
              <p>{t.data.category || (en ? 'Ecommerce' : '通用电商')}</p>
              <span className="ct-image-meta">
                {t.data.output?.ratio || '1:1'} · {t.data.language?.split('。')[0] || (en ? 'Follow task' : '跟随任务')}
              </span>
              <div className="ct-image-actions">
                <Button size="sm" disabled={pending} onClick={() => setEditing(structuredClone(t))}>
                  {words.edit}
                </Button>
                <Button
                  size="sm"
                  disabled={pending}
                  onClick={() =>
                    void perform(async () => {
                      const dest = await open({
                        directory: true,
                        title: en ? 'Choose export folder' : '选择模板导出位置',
                      })
                      if (typeof dest === 'string') library.setNotice(await api.imageTemplateExport(t.id, dest))
                    })
                  }
                >
                  {words.export}
                </Button>
              </div>
            </div>
          </article>
        ))}
      </div>
    </div>
    {preview && <TemplatePagePreview template={preview.template} initialIndex={preview.index} en={en} onClose={() => setPreview(null)} />}
    {editing && <TemplateDialog busy={pending} title={editing.builtin ? words.copy : editing.data.name} closeLabel={en ? 'Close editor' : '关闭模板编辑'} onClose={() => { if (!pending) setEditing(null) }}>
      {pending && <p role="status">{en ? 'Working…' : '处理中…'}</p>}
      {library.error && <p role="alert">{library.error}</p>}
      {library.notice && <p role="status">{library.notice}</p>}
      <fieldset disabled={pending} className="flex min-w-0 flex-col gap-4">
        {field(words.name, editing.data.name, name => patch({ name }))}
        {field(words.category, editing.data.category || '', category => patch({ category }))}
        {field(words.language, editing.data.language || '', language => patch({ language }))}
        {field(words.style, editing.data.style || '', style => patch({ style }), true)}
        <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
          {field(words.ratio, editing.data.output?.ratio || '', ratio => patch({ output: { ...editing.data.output, ratio } }))}
          {field(words.resolution, editing.data.output?.resolution || '', resolution => patch({ output: { ...editing.data.output, resolution } }))}
        </div>
        {editing.data.slots.map((slot, index) => <section key={index} className="flex min-w-0 flex-col gap-3">
          <h3>{index + 1}. {slot.purpose || slot.id}</h3>
          {slot.example && editing.id && <div className="content-template-preview-image"><Example id={editing.id} reference={slot.example} /></div>}
          {field('ID', slot.id, id => patchSlot(index, { id }))}
          {field(words.purpose, slot.purpose || '', purpose => patchSlot(index, { purpose }))}
          {field(words.rule, (editing.data.mode === 'replace' ? slot.prompt : slot.brief) || '', value => patchSlot(index, { [editing.data.mode === 'replace' ? 'prompt' : 'brief']: value }), true)}
          {slot.example && <p className="break-words">{slot.example}</p>}
          <Button disabled={pending || editing.data.slots.length <= 1} onClick={() => patch({ slots: editing.data.slots.filter((_, i) => i !== index) })}>{words.remove}</Button>
        </section>)}
        {editing.data.mode === 'smart' && <Button disabled={pending || editing.data.slots.length >= 30} onClick={() => patch({ slots: [...editing.data.slots, { id: `page-${crypto.randomUUID().slice(0, 6)}`, brief: '' }] })}>{words.add}</Button>}
        <div className="flex flex-wrap gap-3">
          <Button variant="primary" disabled={pending} onClick={() => void perform(async () => { accept(await api.imageTemplateSave(editing)); library.setNotice(words.saved) })}>{editing.builtin ? words.copy : t.save}</Button>
          <Button disabled={pending || !editing.id} onClick={() => void perform(async () => { const dest = await open({ directory: true }); if (typeof dest === 'string') library.setNotice(await api.imageTemplateExport(editing.id, dest)) })}>{words.export}</Button>
          <Button disabled={pending} onClick={() => setEditing(null)}>{t.cancel}</Button>
        </div>
      </fieldset>
    </TemplateDialog>}
  </WorkbenchPage>
}
