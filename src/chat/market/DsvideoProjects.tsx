import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { dsvideoProjectsApi, type DsvideoRegistry, type DsvideoProjectContext } from '../../api/dsvideoProjects'
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
  return <div className="kv-modal-backdrop kv-modal-backdrop--portal" data-tauri-drag-region="false">
    <section className="kv-modal w-full max-w-xl space-y-4 p-5" role="dialog" aria-modal="true" onKeyDown={event => { if (event.key === 'Escape' && !busy) onClose() }} aria-label={zh ? 'Dsvideo 项目' : 'Dsvideo projects'}>
      <h2 className="text-lg font-semibold">{zh ? '选择 Dsvideo 项目' : 'Choose a Dsvideo project'}</h2>
      <p className="text-sm text-neutral-500">{zh ? '插件已内置。首次使用只初始化项目；素材、进度和成果会保存在该目录。' : 'Dsvideo is included. First use prepares the project; assets, progress and outputs stay in its folder.'}</p>
      <div className="max-h-56 space-y-2 overflow-auto custom-scrollbar">
        {registry?.projects.map(project => <div key={project.id} className="flex items-center justify-between gap-3">
          <div className="min-w-0"><strong className="text-sm">{project.name}{registry.current === project.id ? (zh ? '（上次使用）' : ' (last used)') : ''}</strong><p className="break-all text-xs text-neutral-500">{project.path}</p></div>
          <Button size="sm" disabled={busy || !project.available} onClick={() => void choose(project.path, false)}>{project.available ? (zh ? '继续项目' : 'Continue') : (zh ? '目录或配置不可用' : 'Unavailable')}</Button>
        </div>)}
        {registry && !registry.projects.length && <p className="text-sm">{zh ? '还没有 Dsvideo 项目，请选择或创建一个目录。' : 'No Dsvideo projects yet. Choose or create a folder.'}</p>}
      </div>
      <Input value={name} onChange={setName} aria-label={zh ? '项目名称' : 'Project name'} placeholder={zh ? '项目名称（可选）' : 'Project name (optional)'} disabled={busy} />
      <Input autoFocus value={path} onChange={setPath} aria-label={zh ? '项目目录' : 'Project folder'} placeholder={zh ? '已有目录或要创建的绝对路径' : 'Existing folder or a new absolute path'} disabled={busy} />
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} onClick={() => { void open({ directory: true, multiple: false }).then(value => { if (typeof value === 'string') setPath(value) }).catch(e => setError(String(e))) }}>{zh ? '选择文件夹' : 'Choose folder'}</Button>
        <Button variant="primary" disabled={busy || !path.trim()} onClick={() => void choose(path.trim(), true)}>{busy ? (zh ? '准备中…' : 'Preparing…') : (zh ? '初始化并使用' : 'Initialize and use')}</Button>
        <Button disabled={busy} onClick={onClose}>{zh ? '取消' : 'Cancel'}</Button>
      </div>
      {registry?.document && <p className="break-all text-xs text-neutral-500">{zh ? '项目登记文档：' : 'Project index: '}{registry.document}</p>}
      {error && <p role="alert" className="text-sm text-red-500 whitespace-pre-wrap">{error}</p>}
    </section>
  </div>
}
