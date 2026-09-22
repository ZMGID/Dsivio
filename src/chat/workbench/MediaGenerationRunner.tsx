import { useEffect, useRef, useState, type ReactNode } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, type ModelProvider } from '../../api/tauri'
import { Button } from '../../components/Button'
import { useLang } from '../../components/i18n'
import type { MediaKind, MediaTask } from '../../generated/mediaGeneration'
import { videoModel } from '../../data/videoModels'
import { FieldBlock, Input, Select, TextArea } from '../../settings/public/controls'
import { CopyUploadField, type LocalImage } from './copy/CopyUploadField'
import { RequirementComposer } from '../images/RequirementComposer'
import { revokeImages } from './copy/CopyUploadField'
import { useLocalImages } from './image/useLocalImages'

const dataUrl = (blob: Blob) => new Promise<string>((resolve, reject) => {
  const reader = new FileReader()
  reader.onload = () => resolve(String(reader.result))
  reader.onerror = () => reject(new Error('读取图片失败'))
  reader.readAsDataURL(blob)
})
async function readImages(images: LocalImage[]) {
  return Promise.all(images.map(async image => dataUrl(await (await fetch(image.url)).blob())))
}

export interface MediaGenerationView {
  prompt: ReactNode
  assets: ReactNode
  options: ReactNode
  action: ReactNode
  results: ReactNode
  tasks: MediaTask[]
  error: string
  busy: boolean
  reset: () => void
}
export interface MediaImageFieldProps {
  label: string
  files: LocalImage[]
  onChange: (images: LocalImage[]) => void
  onNotice: (message: string) => void
  max: number
  optional?: boolean
  onBusyChange?: (busy: boolean) => void
}

/** Both cloud and local models use the same generation API and saved task list. */
export function MediaGenerationRunner({ provider, model, kind, render, renderImages, onSubmitted }: {
  provider?: ModelProvider
  model: string
  kind: MediaKind
  render?: (view: MediaGenerationView) => ReactNode
  renderImages?: (props: MediaImageFieldProps) => ReactNode
  onSubmitted?: () => void
}) {
  const zh = useLang() === 'zh'
  const workflow = provider?.request.comfy?.workflows.find(w => w.id === model)
  const profile = kind === 'video' ? videoModel(model) : undefined
  const [prompt, setPrompt] = useState('')
  const [values, setValues] = useState<Record<string, string>>({})
  const [files, setFiles] = useState<Record<string, string>>({})
  const [images, setImages] = useLocalImages()
  const [lastFrame, setLastFrame] = useLocalImages()
  const [mode, setMode] = useState(profile?.modes.includes('text') ? 'text' : profile?.modes[0] || 'text')
  const [tasks, setTasks] = useState<MediaTask[]>([])
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [reload, setReload] = useState(0)
  const mounted = useRef(true)
  const choice = `${provider?.id || ''}:${model}`
  const currentChoice = useRef(choice)
  currentChoice.current = choice
  useEffect(() => {
    setValues({}); setFiles({})
    const next = kind === 'video' ? videoModel(model) : undefined
    setMode(next?.modes.includes('text') ? 'text' : next?.modes[0] || 'text')
  }, [choice, kind, model])
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => {
    if (!provider) { setTasks([]); setLoading(false); return }
    const providerId = provider.id
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    async function poll() {
      try {
        const loaded = await api.listMediaTasks(providerId, model)
        if (!active) return
        setTasks(loaded)
        if (loaded.some(task => task.status === 'running')) timer = setTimeout(() => void poll(), 2500)
      } catch (failure) { if (active) setError(String(failure)) }
      finally { if (active) setLoading(false) }
    }
    setLoading(true); setError(''); void poll()
    return () => { active = false; if (timer) clearTimeout(timer) }
  }, [provider, model, reload])

  async function submit() {
    if (pending.current || !provider) return
    const submittedChoice = choice
    pending.current = true; setBusy(true); setError('')
    try {
      const options: Record<string, unknown> = {}
      let references: string[] = []
      if (workflow) {
        for (const binding of workflow.inputs) {
          const key = `${binding.nodeId}:${binding.input}`
          const value = values[key]
          if (value === undefined) continue
          if (binding.kind === 'number' && (!value.trim() || !Number.isFinite(Number(value)))) throw new Error(`${binding.label}: ${zh ? '请输入有效数字' : 'Enter a valid number'}`)
          options[key] = binding.kind === 'number' ? Number(value) : value
        }
      } else if (kind === 'image') {
        references = await readImages(images)
        if (values.ratio) options.aspect_ratio = values.ratio
        if (values.size) options.size = values.size
      } else {
        if (mode === 'text' && images.length) throw new Error(zh ? '已添加图片，请选择首帧或参考素材模式' : 'Choose an image input mode to use your uploaded images')
        for (const key of ['duration', 'resolution', 'ratio']) if (values[key]) options[key] = key === 'duration' ? Number(values[key]) : values[key]
        if (values.audio) options.generateAudio = values.audio === 'true'
        if (mode === 'image' || mode === 'frames') {
          if (images.length > 1) throw new Error(zh ? '首帧只能使用一张图片，请移除多余图片' : 'Use one image for the first frame')
          if (!images.length) throw new Error(zh ? '请上传首帧图片' : 'Upload a first frame')
          options.firstFrame = (await readImages(images))[0]
          if (mode === 'frames') {
            if (!lastFrame.length) throw new Error(zh ? '请上传尾帧图片' : 'Upload a last frame')
            options.lastFrame = (await readImages(lastFrame))[0]
          }
        }
        if (mode === 'reference') {
          options.referenceImages = await readImages(images)
          for (const key of ['referenceVideos', 'referenceAudios']) if (values[key]?.trim()) options[key] = values[key].split('\n').map(v => v.trim()).filter(Boolean)
        }
      }
      const task = await api.startMediaGeneration({ providerId: provider.id, model, kind, prompt, images: references, options })
      if (!mounted.current || currentChoice.current !== submittedChoice) return
      setTasks(current => [task, ...current.filter(item => item.id !== task.id)])
      setReload(value => value + 1)
      onSubmitted?.()
    } catch (failure) { if (mounted.current && currentChoice.current === submittedChoice) setError(String(failure)) }
    finally { pending.current = false; if (mounted.current) setBusy(false) }
  }
  const pick = (key: string, label: string, choices: (string | number)[]) => <FieldBlock key={key} label={label}><Select className="w-full" ariaLabel={label} value={values[key] || ''} disabled={busy} options={[{ value: '', label: zh ? '模型默认值' : 'Model default' }, ...choices.map(value => ({ value: String(value), label: String(value) }))]} onChange={value => setValues(v => ({ ...v, [key]: value }))} /></FieldBlock>
  const modeNames: Record<string, string> = zh ? { text: '文字生成', image: '首帧图片', frames: '首尾帧', reference: '参考素材' } : { text: 'Text', image: 'First frame', frames: 'First and last frames', reference: 'References' }
  const ImageField = renderImages || CopyUploadField
  const imageBusy = (value: boolean) => { pending.current = value; if (mounted.current) setBusy(value) }
  const promptField = workflow ? <>{workflow.inputs.map(binding => {
          const key = `${binding.nodeId}:${binding.input}`
          const original = String(workflow.graph[binding.nodeId]?.inputs[binding.input] ?? '')
          return <FieldBlock key={key} label={binding.label}>
            {binding.kind === 'image' ? <>
              <Input aria-label={binding.label} type="file" accept="image/png,image/jpeg,image/webp" value="" onChange={() => {}} disabled={busy} onInput={event => {
                const file = event.currentTarget.files?.[0]
                if (!file || pending.current) return
                if (file.size > 30 * 1024 * 1024) { setError(zh ? '参考图不能超过 30 MB' : 'Reference images must be under 30 MB'); return }
                pending.current = true; setBusy(true)
                void dataUrl(file).then(data => { if (mounted.current) { setValues(v => ({ ...v, [key]: data })); setFiles(v => ({ ...v, [key]: file.name })) } }).catch(failure => { if (mounted.current) setError(String(failure)) }).finally(() => { pending.current = false; if (mounted.current) setBusy(false) })
              }} />
              <p className="workbench-page-sub [overflow-wrap:anywhere]">{files[key] || original}</p>
            </> : binding.kind === 'number' ? <Input aria-label={binding.label} type="number" value={values[key] ?? original} onChange={value => setValues(v => ({ ...v, [key]: value }))} /> :
              <label className="block"><span className="sr-only">{binding.label}</span><TextArea value={values[key] ?? original} onChange={value => setValues(v => ({ ...v, [key]: value }))} rows={3} /></label>}
          </FieldBlock>
        })}</> : render ? <RequirementComposer label={zh ? '这次要拍什么' : 'Describe your video'} value={prompt} onChange={setPrompt} placeholder={zh ? '例如：让背包在自然光下缓慢转动，展示正面细节，不要口播。' : 'Describe the scene and movement you want.'} /> : <label className="block"><span className="kv-row-label">{zh ? '提示词' : 'Prompt'}</span><TextArea value={prompt} onChange={setPrompt} rows={5} /></label>
  const assets = workflow ? null : <>{(renderImages || kind === 'image' || mode !== 'text') && <ImageField label={mode === 'frames' || mode === 'image' ? (zh ? '首帧图片' : 'First frame') : (zh ? '参考图片' : 'Reference images')} onBusyChange={imageBusy} files={images} onChange={setImages} onNotice={setError} optional={kind === 'image' || mode === 'reference'} max={kind === 'image' ? 4 : mode === 'reference' ? (profile && 'referenceLimits' in profile ? profile.referenceLimits?.images ?? 4 : 4) : renderImages && mode === 'text' ? 4 : 1} />}
          {mode === 'frames' && <ImageField label={zh ? '尾帧图片' : 'Last frame'} onBusyChange={imageBusy} files={lastFrame} onChange={setLastFrame} onNotice={setError} max={1} />}
          {mode === 'reference' && ['referenceVideos', 'referenceAudios'].map(key => <label key={key} className="block"><span className="kv-row-label">{key === 'referenceVideos' ? (zh ? '参考视频 URL（每行一个）' : 'Reference video URLs (one per line)') : (zh ? '参考音频 URL（每行一个）' : 'Reference audio URLs (one per line)')}</span><TextArea value={values[key] || ''} onChange={value => setValues(v => ({ ...v, [key]: value }))} rows={2} /></label>)}</>
  const options = workflow ? null : <>{kind === 'video' && profile && <FieldBlock label={zh ? '输入素材' : 'Input'}><Select className="w-full" ariaLabel={zh ? '输入素材' : 'Input'} value={mode} options={profile.modes.map(value => ({ value, label: modeNames[value] || value }))} onChange={setMode} /></FieldBlock>}<div className={render ? "flex min-w-0 flex-col gap-3" : "grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-3"}>
            {kind === 'video' && profile ? <>
              {profile.durations.length > 0 && pick('duration', zh ? '时长（秒）' : 'Duration (seconds)', profile.durations)}
              {profile.resolutions.length > 0 && pick('resolution', zh ? '分辨率' : 'Resolution', profile.resolutions)}
              {profile.ratios.length > 0 && pick('ratio', zh ? '比例' : 'Ratio', profile.ratios)}
            </> : kind === 'image' && <>{pick('ratio', zh ? '比例' : 'Ratio', ['1:1', '16:9', '9:16', '4:3', '3:4'])}{pick('size', zh ? '尺寸' : 'Size', ['1K', '2K', '4K'])}</>}
          </div>
          {profile && 'audioToggle' in profile && profile.audioToggle && <FieldBlock label={zh ? '生成声音' : 'Generate audio'}><Select className="w-full" ariaLabel={zh ? '生成声音' : 'Generate audio'} value={values.audio || ''} options={[{ value: '', label: zh ? '模型默认值' : 'Model default' }, { value: 'true', label: zh ? '开启' : 'On' }, { value: 'false', label: zh ? '关闭' : 'Off' }]} onChange={value => setValues(v => ({ ...v, audio: value }))} /></FieldBlock>}</>
  const action = <Button variant="primary" disabled={!provider || busy || loading || (!workflow && !prompt.trim())} onClick={() => void submit()}>{busy ? (zh ? '正在提交…' : 'Submitting…') : (zh ? '开始生成' : 'Generate')}</Button>
  const results = <section className={render ? "vs-panel" : "workbench-card-block"}>
      <div className="flex min-w-0 items-center justify-between gap-4"><h2 className="workbench-card-block-title">{zh ? '生成记录' : 'Generations'}</h2><Button size="sm" disabled={loading || busy} onClick={() => setReload(value => value + 1)}>{zh ? '刷新' : 'Refresh'}</Button></div>
      {!tasks.length && <p className="workbench-page-sub">{loading ? (zh ? '正在加载…' : 'Loading…') : (zh ? '还没有生成记录。' : 'No generations yet.')}</p>}
      <ul className="divide-y divide-border">{tasks.map(task => <li key={task.id} className="flex min-w-0 flex-col gap-3 py-4">
        <div className="flex min-w-0 flex-wrap justify-between gap-2"><span className="kv-row-label">{new Date(task.createdAt).toLocaleString()}</span><span className="kv-row-label" role="status">{task.status === 'running' ? (zh ? '生成中' : 'Generating') : task.status === 'succeeded' ? (zh ? '完成' : 'Completed') : (zh ? '失败' : 'Failed')}</span></div>
        {task.error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{task.error}</p>}
        {task.canResume && <Button size="sm" className="self-start" onClick={() => void api.getMediaTask(task.id, true).then(() => setReload(value => value + 1)).catch(failure => setError(String(failure)))}>{zh ? '恢复查询／下载' : 'Resume query / download'}</Button>}
        <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">{task.outputs.map(output => <div key={output.path} className="min-w-0">
          {output.mime.startsWith('image/') ? <img className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.path)} alt={workflow?.name || model} loading="lazy" /> : <video className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.path)} controls preload="metadata" />}
          <Button size="sm" className="mt-2" onClick={() => void api.openLocalFile(output.path).catch(failure => setError(String(failure)))}>{zh ? '打开文件' : 'Open file'}</Button>
        </div>)}</div>
      </li>)}</ul>
    </section>
  const reset = () => {
    if (pending.current) return
    revokeImages(images); revokeImages(lastFrame)
    setPrompt(''); setValues({}); setFiles({}); setImages([]); setLastFrame([]); setError('')
  }
  if (render) return render({ prompt: promptField, assets, options, action, results, tasks, error, busy, reset })
  return <div className="flex min-w-0 flex-col gap-4">
    <section className="workbench-card-block">
      <h2 className="workbench-card-block-title [overflow-wrap:anywhere]">{workflow?.name || (kind === 'image' ? (zh ? '生成图片' : 'Generate image') : (zh ? '生成视频' : 'Generate video'))}</h2>
      <fieldset disabled={busy} className="flex min-w-0 flex-col gap-4">{promptField}{assets}{options}</fieldset>
      <div className="pt-4">{action}</div>
    </section>
    {results}
    {error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{error}</p>}
  </div>
}
