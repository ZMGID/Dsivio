import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { dsvideoProjectsApi, type DsvideoRegistry, type DsvideoProjectContext } from '../../api/dsvideoProjects'
import { confirmDialog } from '../../components/dialogQueue'
import { Button } from '../../components/Button'
import { Input } from '../../settings/public/controls'
import type { Lang } from '../../components/i18n'

export function DsvideoProjects({ lang, onClose, onChoose }: { lang: Lang; onClose: () => void; onChoose: (project: DsvideoProjectContext) => void }) {
  const zh = lang === 'zh'
  const active = useRef(true)
  const [registry, setRegistry] = useState<DsvideoRegistry | null>(null)
  const [path, setPath] = useState('')
  const [name, setName] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [creating, setCreating] = useState(false)
  useEffect(() => {
    let disposed = false
    active.current = true
    void dsvideoProjectsApi.list().then(value => { if (!disposed) setRegistry(value) }).catch(e => { if (!disposed) setError(String(e)) })
    return () => { disposed = true; active.current = false }
  }, [])
  const choose = async (path: string, initialize: boolean) => {
    const startingHash = window.location.hash
    setBusy(true); setError('')
    try {
      if (initialize) await dsvideoProjectsApi.init(path, name)
      const project = await dsvideoProjectsApi.bind(path)
      if (active.current && window.location.hash === startingHash) onChoose(project)
    } catch (e) { if (active.current) setError(String(e)) } finally { if (active.current) setBusy(false) }
  }
  const remove = async (project: { name: string; path: string }) => {
    setBusy(true); setError('')
    try {
      const confirmed = await confirmDialog({
        message: zh ? `删除「${project.name}」的 Dsvideo 项目登记？项目目录、素材、进度和成片，以及已有对话都会保留。` : `Remove "${project.name}" from Dsvideo projects? The folder, assets, progress, outputs and existing conversations will be kept.`,
        confirmLabel: zh ? '删除登记' : 'Remove', danger: true,
      })
      if (!confirmed || !active.current) return
      const next = await dsvideoProjectsApi.remove(project.path)
      if (active.current) { setRegistry(next); setCreating(false) }
    } catch (e) { if (active.current) setError(String(e)) }
    finally { if (active.current) setBusy(false) }
  }
  const hasProjects = Boolean(registry?.projects.length)
  const showForm = registry !== null && (!hasProjects || creating)
  return <div className="kv-modal-backdrop kv-modal-backdrop--portal" data-tauri-drag-region="false">
    <section className="kv-modal w-full max-w-xl space-y-4 p-5" role="dialog" aria-modal="true" onKeyDown={event => { if (event.key === 'Escape' && !busy) onClose() }} aria-label={zh ? 'Dsvideo 项目' : 'Dsvideo projects'}>
      <h2 className="text-lg font-semibold">{zh ? '选择 Dsvideo 项目' : 'Choose a Dsvideo project'}</h2>
      <p className="kv-row-desc">{hasProjects ? (zh ? '选择已有项目继续制作，素材和进度会保留在项目目录中。' : 'Continue an existing project. Assets and progress stay in its folder.') : (zh ? '选择一个项目目录，保存后续的素材、进度和成果。' : 'Choose a project folder for assets, progress and outputs.')}</p>
      {registry === null && !error && <p className="kv-row-desc" role="status">{zh ? '正在读取项目…' : 'Loading projects…'}</p>}
      {(!creating || !hasProjects) && <div className="max-h-56 space-y-2 overflow-auto custom-scrollbar">
        {registry?.projects.map(project => <div key={project.id} className="flex items-center justify-between gap-3">
          <div className="min-w-0"><strong className="text-sm">{project.name}{registry.current === project.id ? (zh ? '（上次使用）' : ' (last used)') : ''}</strong><p className="kv-row-desc break-all">{project.path}</p></div>
          <div className="flex shrink-0 items-center gap-2">
          <Button size="sm" variant={registry.current === project.id && project.available ? 'primary' : 'default'} disabled={busy || !project.available} onClick={() => void choose(project.path, false)}>{project.available ? (zh ? '继续项目' : 'Continue') : (zh ? '目录或配置不可用' : 'Unavailable')}</Button>
          <Button size="sm" variant="danger" aria-label={`${zh ? '删除项目' : 'Remove project'} ${project.name}`} disabled={busy} onClick={() => void remove(project)}>{zh ? '删除' : 'Remove'}</Button>
          </div>
        </div>)}
        {registry && !registry.projects.length && <p className="text-sm">{zh ? '还没有 Dsvideo 项目，请选择或创建一个目录。' : 'No Dsvideo projects yet. Choose or create a folder.'}</p>}
      </div>}
      {showForm && <>
        {hasProjects && <h3 className="kv-row-label">{zh ? '新建项目' : 'New project'}</h3>}
        <Input value={name} onChange={setName} aria-label={zh ? '项目名称' : 'Project name'} placeholder={zh ? '项目名称（可选）' : 'Project name (optional)'} disabled={busy} />
        <Input autoFocus value={path} onChange={setPath} aria-label={zh ? '项目目录' : 'Project folder'} placeholder={zh ? '已有目录或要创建的绝对路径' : 'Existing folder or a new absolute path'} disabled={busy} />
        <div className="flex flex-wrap gap-2">
          <Button disabled={busy} onClick={() => { void open({ directory: true, multiple: false }).then(value => { if (active.current && typeof value === 'string') setPath(value) }).catch(e => { if (active.current) setError(String(e)) }) }}>{zh ? '选择文件夹' : 'Choose folder'}</Button>
          <Button variant="primary" disabled={busy || !path.trim()} onClick={() => void choose(path.trim(), true)}>{busy ? (zh ? '准备中…' : 'Preparing…') : (zh ? '初始化并使用' : 'Initialize and use')}</Button>
        </div>
      </>}
      <div className="flex flex-wrap gap-2">
        {hasProjects && <Button disabled={busy} onClick={() => { setCreating(value => !value); setError('') }}>{creating ? (zh ? '返回已有项目' : 'Back to projects') : (zh ? '新建项目' : 'New project')}</Button>}
        <Button disabled={busy} onClick={onClose}>{zh ? '取消' : 'Cancel'}</Button>
      </div>
      {error && <p role="alert" className="text-sm text-red-500 whitespace-pre-wrap">{error}</p>}
    </section>
  </div>
}
