import { useEffect, useRef, useState } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import { FileImage, Plus, X } from 'lucide-react'
import { isTauriRuntime } from '../../api/tauri'
import { Button, IconButton } from '../../components/Button'
import { loadAttachmentDataUrl } from '../attachmentPreview'
import { useChatRouteActive } from '../chatRouteVisibility'
import type { MediaImageFieldProps } from '../workbench/MediaGenerationRunner'

/** The original studio drop area; native file reads reuse the existing image reader. */
export function VideoImageUpload({ label, files, max, onChange, onNotice, onBusyChange }: MediaImageFieldProps) {
  const area = useRef<HTMLDivElement>(null)
  const input = useRef<HTMLInputElement>(null)
  const mounted = useRef(true)
  const pending = useRef(false)
  const [hover, setHover] = useState(false)
  const [busy, setBusy] = useState(false)
  const active = useChatRouteActive()
  const blocked = () => pending.current || Boolean(area.current?.closest('fieldset:disabled'))
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])

  async function add(read: () => Promise<{ name: string; url: string }[]>) {
    if (blocked()) return
    pending.current = true; setBusy(true); onBusyChange?.(true)
    try {
      const added = await read()
      if (!mounted.current) return
      if (files.length + added.length > max) throw new Error(`最多添加 ${max} 张图片`)
      onChange([...files, ...added.map(file => ({ ...file, id: crypto.randomUUID() }))])
      onNotice('')
    } catch (error) { if (mounted.current) onNotice(String(error)) }
    finally { pending.current = false; onBusyChange?.(false); if (mounted.current) setBusy(false) }
  }
  const readPaths = async (paths: string[]) => {
    if (paths.some(path => !/\.(png|jpe?g|webp)$/i.test(path))) throw new Error('请使用 PNG、JPG 或 WebP 图片')
    if (files.length + paths.length > max) throw new Error(`最多添加 ${max} 张图片`)
    return Promise.all(paths.map(async path => {
      const name = path.split(/[\\/]/).pop() || '参考图'
      const url = await loadAttachmentDataUrl({ path, name, type: 'image' })
      if (!url) throw new Error(`无法读取图片：${name}`)
      return { name, url }
    }))
  }
  const drop = useRef({ active, readPaths, add })
  drop.current = { active, readPaths, add }
  useEffect(() => {
    if (!isTauriRuntime()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    void getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (disposed || !drop.current.active || blocked()) return
      if (payload.type === 'leave') { setHover(false); return }
      const rect = area.current?.getBoundingClientRect()
      if (!rect) return
      const scale = window.devicePixelRatio || 1
      const { x, y } = payload.position
      const inside = x / scale >= rect.left && x / scale <= rect.right && y / scale >= rect.top && y / scale <= rect.bottom
      setHover(inside && payload.type !== 'drop')
      if (inside && payload.type === 'drop') void drop.current.add(() => drop.current.readPaths(payload.paths))
    }).then(stop => { if (disposed) stop(); else unlisten = stop }).catch(error => { if (!disposed) onNotice(String(error)) })
    return () => { disposed = true; unlisten?.() }
  }, [onNotice])

  return <div ref={area}>
    <div className="vs-media-head"><h3>{label}</h3><small>也可以只写要求，不放图</small></div>
    <div className={`vs-drop${files.length ? ' is-upload-area--filled' : ''}${hover ? ' is-drop-active' : ''}`}
      aria-label={`${label}投放区`}
      onDragOver={event => { event.preventDefault(); if (!blocked()) setHover(true) }}
      onDragLeave={() => setHover(false)}
      onDrop={event => {
        event.preventDefault(); setHover(false)
        if (isTauriRuntime()) return
        const dropped = Array.from(event.dataTransfer.files)
        void add(() => readFiles(dropped))
      }}>
      {files.length ? <div className="vs-assets">{files.map(file => <div key={file.id}>
        <img src={file.url} alt={file.name} />
        <IconButton label={`移除 ${file.name}`} disabled={busy} onClick={() => { URL.revokeObjectURL(file.url); onChange(files.filter(item => item.id !== file.id)) }}><X size={13} /></IconButton>
      </div>)}</div> : <div className="vs-drop-empty"><span className="vs-drop-mark"><FileImage size={22} strokeWidth={1.5} /></span><strong>{hover ? '松开即可导入' : '把商品图片拖到这里'}</strong><span>PNG / JPG / WebP</span></div>}
    </div>
    <div className="vs-drop-bar"><small>{files.length ? '还可以把图片继续拖进来' : '添加图片后，在右侧选择首帧或参考素材模式'}</small>
      <Button size="sm" disabled={busy} onClick={() => {
        if (!isTauriRuntime()) { input.current?.click(); return }
        void add(async () => {
          const paths = await open({ multiple: max > 1, filters: [{ name: '商品图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] })
          return paths ? readPaths(Array.isArray(paths) ? paths : [paths]) : []
        })
      }}><Plus size={14} />{busy ? '正在读取…' : files.length ? '继续添加参考图' : '选择商品图片'}</Button>
    </div>
    <input ref={input} type="file" aria-label="选择商品图片文件" hidden accept="image/png,image/jpeg,image/webp" multiple={max > 1}
      onChange={event => { const selected = Array.from(event.target.files || []); event.target.value = ''; void add(() => readFiles(selected)) }} />
  </div>
}
async function readFiles(files: File[]) {
  return Promise.all(files.map(file => new Promise<{ name: string; url: string }>((resolve, reject) => {
    if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 30 * 1024 * 1024) { reject(new Error('请使用不超过 30 MB 的 PNG、JPG 或 WebP 图片')); return }
    const reader = new FileReader()
    reader.onload = () => resolve({ name: file.name, url: String(reader.result) })
    reader.onerror = () => reject(new Error('图片读取失败'))
    reader.readAsDataURL(file)
  })))
}
