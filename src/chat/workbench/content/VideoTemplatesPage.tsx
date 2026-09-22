import { Film } from 'lucide-react'
import { TemplateDialog } from './TemplateDialog'
import './templateLibrary.css'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useLang, useT } from '../../../components/i18n'
import { Input, TextArea } from '../../../settings/public/controls'
import { WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { useTemplateLibrary } from './useTemplateLibrary'

export function VideoTemplatesPage() {
  const t = useT()
  const en = useLang() === 'en'
  const library = useTemplateLibrary(api.videoTemplatesList)
  const { editing, setEditing, pending, perform, accept } = library
  const words = en ? { refresh: 'Refresh', import: 'Import JSON', export: 'Export', name: 'Name', script: 'Script', duration: 'Duration (seconds)', ratio: 'Aspect ratio', saved: 'Template saved', empty: 'No templates', edit: 'View / edit', reference: 'Reference · output not verified', generation: 'Generation template', time: 'Time', purpose: 'Purpose', action: 'Action', camera: 'Camera', add: 'Add shot', remove: 'Remove shot' } : { refresh: '刷新', import: '导入 JSON', export: '导出', name: '模板名称', script: '剧本', duration: '时长（秒）', ratio: '画面比例', saved: '模板已保存', empty: '暂无模板', edit: '查看 / 编辑', reference: '参考模板 · 未验证成片', generation: '生成模板', time: '时间', purpose: '用途', action: '动作', camera: '镜头', add: '添加镜头', remove: '移除镜头' }
  const field = (name: string, value: string, change: (value: string) => void, multiline = false) => <label className="flex min-w-0 flex-col gap-2"><span>{name}</span>{multiline ? <TextArea value={value} onChange={change} rows={6} /> : <Input value={value} onChange={change} />}</label>
  return <WorkbenchPage crumb={t.workbenchGroupContent} title={t.workbenchNavVideoTemplates} actions={<>
    <Button disabled={pending} onClick={() => void library.refresh()}>{words.refresh}</Button>
    <Button disabled={pending} onClick={() => void perform(async () => { const path = await open({ filters: [{ name: 'JSON', extensions: ['json'] }] }); if (typeof path === 'string') accept(await api.videoTemplateImport(path)) })}>{words.import}</Button>
  </>}>
    {!editing && library.error && <p role="alert" className="workbench-inline-note">{library.error}</p>}
    {!editing && library.notice && <p role="status">{library.notice}</p>}
    {pending && <p role="status">{en ? 'Working…' : '处理中…'}</p>}
    <div className="content-video-templates">
      {!pending && !library.items.length && <WorkbenchEmpty title={words.empty} />}
      <div className="vs-template-grid">
        {library.items.map(template => <article className="vs-template" key={template.id}>
          <div className="vs-template-preview">
            <Film size={22} />
            <span>{template.spec.duration_seconds ? `${template.spec.duration_seconds} ${en ? 'seconds' : '秒'}` : (en ? 'Reference script' : '参考剧本')} · {String(template.spec.aspect_ratio || (en ? 'Custom' : '自定义'))}</span>
            {template.shots.slice(0, 3).map((shot, index) => <div key={index}><small>{String(shot.time || '')}</small><span>{String(shot.purpose || shot.action || '')}</span></div>)}
          </div>
          <div className="vs-template-body">
            <small>{template.kind === 'generation' ? words.generation : words.reference}</small>
            <h3>{template.name}</h3>
            <details>
              <summary>{en ? 'View script' : '查看剧本'}</summary>
              <pre className="custom-scrollbar">{template.script || template.shots.map(shot => `${shot.time || ''} ${shot.purpose || ''}\n${shot.action || ''}\n${shot.camera || ''}`).join('\n\n')}</pre>
            </details>
            <div className="vs-template-actions">
              <Button size="sm" disabled={pending} onClick={() => setEditing(structuredClone(template))}>{words.edit}</Button>
              <Button size="sm" disabled={pending} onClick={() => void perform(async () => { const dest = await open({ directory: true }); if (typeof dest === 'string') library.setNotice(await api.videoTemplateExport(template.id, dest)) })}>{words.export}</Button>
            </div>
          </div>
        </article>)}
      </div>
    </div>
    {editing && <TemplateDialog busy={pending} title={editing.name} closeLabel={en ? 'Close editor' : '关闭模板编辑'} onClose={() => { if (!pending) setEditing(null) }}>
      <p>{editing.kind === 'generation' ? words.generation : words.reference}</p>
      {pending && <p role="status">{en ? 'Working…' : '处理中…'}</p>}
      {library.error && <p role="alert">{library.error}</p>}
      {library.notice && <p role="status">{library.notice}</p>}
      <fieldset disabled={pending} className="flex min-w-0 flex-col gap-4">
        {field(words.name, editing.name, name => setEditing({ ...editing, name }))}
        {field(words.script, editing.script, script => setEditing({ ...editing, script }), true)}
        <div className="grid min-w-0 grid-cols-1 gap-4 sm:grid-cols-2">
          <label className="flex min-w-0 flex-col gap-2"><span>{words.duration}</span><Input type="number" min="0" value={String(editing.spec.duration_seconds ?? '')} onChange={duration => setEditing({ ...editing, spec: { ...editing.spec, duration_seconds: duration === '' ? null : Number(duration) } })} /></label>
          {field(words.ratio, String(editing.spec.aspect_ratio || ''), aspect_ratio => setEditing({ ...editing, spec: { ...editing.spec, aspect_ratio } }))}
        </div>
        {editing.shots.map((shot, index) => <section key={index} className="flex min-w-0 flex-col gap-3">
          <h3>{index + 1}. {String(shot.purpose || '')}</h3>
          {(['time', 'purpose', 'action', 'camera'] as const).map(key => <div key={key}>{field(words[key], String(shot[key] || ''), value => setEditing({ ...editing, shots: editing.shots.map((s, i) => i === index ? { ...s, [key]: value } : s) }), key === 'action')}</div>)}
          <Button disabled={pending} onClick={() => setEditing({ ...editing, shots: editing.shots.filter((_, i) => i !== index) })}>{words.remove}</Button>
        </section>)}
        <Button disabled={pending} onClick={() => setEditing({ ...editing, shots: [...editing.shots, { time: '', purpose: '', action: '', camera: '' }] })}>{words.add}</Button>
        <div className="flex flex-wrap gap-3">
          <Button variant="primary" disabled={pending} onClick={() => void perform(async () => { accept(await api.videoTemplateSave(editing)); library.setNotice(words.saved) })}>{t.save}</Button>
          <Button disabled={pending} onClick={() => void perform(async () => { const dest = await open({ directory: true }); if (typeof dest === 'string') library.setNotice(await api.videoTemplateExport(editing.id, dest)) })}>{words.export}</Button>
          <Button disabled={pending} onClick={() => setEditing(null)}>{t.cancel}</Button>
        </div>
      </fieldset>
    </TemplateDialog>}
  </WorkbenchPage>
}
