import { useEffect, useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { loadAttachmentDataUrl, openAttachment } from '../../attachmentPreview'
import { api, isTauriRuntime } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { Input } from '../../../settings/public/controls'
import type { WorkflowAsset } from './workflowConfig'
import { STORE_IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'

const WORKFLOW_VIDEO_EXTENSIONS = ['mp4', 'mov', 'webm']

/** Native paths reuse chat attachment inspection; no File/blob/base64 enters drafts. */
export function WorkflowAssets({ assets, onChange, many = false, video = false, disabled = false }: {
  assets: WorkflowAsset[]; onChange: (assets: WorkflowAsset[]) => void; many?: boolean; video?: boolean; disabled?: boolean
}) {
  const [busy, setBusy] = useState(false), [error, setError] = useState('')
  const [available, setAvailable] = useState<string[]>([])
  const [previews, setPreviews] = useState<Record<string, string>>({})
  const change = useRef(onChange)
  change.current = onChange
  const assetsRef = useRef(assets)
  assetsRef.current = assets
  const mounted = useRef(true), pending = useRef(false)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  const paths = JSON.stringify(assets.map(a => a.path))
  useEffect(() => {
    let active = true
    setAvailable([]); setPreviews({})
    const pathsToCheck = JSON.parse(paths) as string[]
    if (isTauriRuntime()) void api.chatInspectAttachmentPaths(pathsToCheck.filter(Boolean)).then(async items => {
      if (!active) return
      const valid = items.filter(item => item.type === (video ? 'video' : 'image'))
      setAvailable(valid.map(item => item.path))
      if (!video) {
        const images = await Promise.all(valid.map(async item => [item.path, await loadAttachmentDataUrl({ ...item, type: 'image' })] as const))
        if (active) setPreviews(Object.fromEntries(images.filter((item): item is readonly [string, string] => !!item[1])))
      }
    }).catch(() => { /* A missing or inaccessible path stays visibly unavailable. */ })
    return () => { active = false }
  }, [paths, video])
  async function guarded(work: () => Promise<void>) {
    if (pending.current || disabled) return
    pending.current = true; setBusy(true); setError('')
    try { await work() }
    catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { pending.current = false; if (mounted.current) setBusy(false) }
  }
  async function adopt(selected: string[], baseline: string[]) {
    const items = await api.chatInspectAttachmentPaths(selected)
    if (!mounted.current) return
    if (items.some(item => item.type !== (video ? 'video' : 'image')) || !items.length) throw new Error('素材不可访问或类型不匹配，请重新选择')
    const current = assetsRef.current
    const removed = new Set(baseline.filter(path => !current.some(asset => asset.path === path)))
    const next = current.map(asset => {
      const picked = items.find(item => item.path === asset.path)
      return picked ? { ...asset, name: picked.name } : asset
    })
    for (const item of items) {
      if (removed.has(item.path) || next.some(asset => asset.path === item.path)) continue
      next.push({ path: item.path, name: item.name, description: '' })
    }
    change.current(next)
  }
  const pick = () => guarded(async () => {
    const baseline = assetsRef.current.map(asset => asset.path)
    if (!isTauriRuntime()) throw new Error('请在桌面应用中选择本机素材')
    const selected = await open({ multiple: many, filters: [{ name: video ? '视频' : '图片', extensions: video ? WORKFLOW_VIDEO_EXTENSIONS : [...STORE_IMAGE_EXTENSIONS] }] })
    if (!selected || !mounted.current) return
    await adopt(Array.isArray(selected) ? selected : [selected], baseline)
  })
  const zone = useRef<HTMLDivElement>(null)
  const over = useFileDrop(zone, video ? WORKFLOW_VIDEO_EXTENSIONS : STORE_IMAGE_EXTENSIONS, (accepted, rejected) => {
    if (accepted.length > 0) void guarded(() => adopt(many ? accepted : accepted.slice(0, 1), assetsRef.current.map(asset => asset.path)))
    else if (rejected.length > 0) setError('只能拖入此类文件：' + (video ? WORKFLOW_VIDEO_EXTENSIONS : STORE_IMAGE_EXTENSIONS).join(' / '))
  }, disabled)
  return <div ref={zone} className={`workbench-flow-fields workbench-drop-zone${over ? ' is-drop-over' : ''}`}>
    <p className="workbench-page-sub">素材保留在本机，移动文件后需重新选择。</p>
    {assets.map((asset, index) => <div key={`${asset.path}:${index}`} className="workbench-flow-asset">
      <span>{asset.name || '未命名素材'}</span>
      <span className="workbench-page-sub">{asset.path || '无持久引用'}</span>
      {available.includes(asset.path) ? video
        ? <Button size="sm" onClick={() => void openAttachment({ ...asset, type: 'video' }).catch(failure => { if (mounted.current) setError(String(failure)) })}>查看 {asset.name}</Button>
        : previews[asset.path] ? <img src={previews[asset.path]} alt={asset.description || asset.name} onError={() => setAvailable(items => items.filter(path => path !== asset.path))} /> : <p>预览未加载或不可读取，请重新选择</p>
        : <p role="status">素材未验证或不可访问，请重新选择</p>}
      <label><span>素材描述</span><Input aria-label={`素材描述 ${index + 1}`} value={asset.description} disabled={disabled || busy} onChange={description => onChange(assets.map((a, i) => i === index ? { ...a, description } : a))} /></label>
      <Button size="sm" disabled={disabled || busy} onClick={() => onChange(assets.filter((_, i) => i !== index))}>移除 {asset.name}</Button>
    </div>)}
    <Button size="sm" disabled={disabled || busy} onClick={() => void pick()}>{busy ? '正在选择…' : assets.length ? '更换素材' : many ? '选择多张图片' : video ? '选择视频' : '选择图片'}</Button>
    {error && <p role="alert">{error}</p>}
  </div>
}
