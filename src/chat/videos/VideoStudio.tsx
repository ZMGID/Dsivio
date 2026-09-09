import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import {
  Clapperboard,
  FileImage,
  Film,
  FolderOpen,
  History,
  Layers,
  Plus,
  RefreshCw,
  ScanSearch,
  Settings2,
  WandSparkles,
  X,
} from 'lucide-react'
import { api, isTauriRuntime } from '../../api/tauri'
import { Button, IconButton } from '../../components/Button'
import { Field, StudioSelect } from '../images/StudioPanels'
import {
  newVideoBrief,
  videoStatus,
  videoRatios,
  languages,
  type VideoBootstrap,
  type VideoBrief,
  type VideoTask,
  type VideoTemplate,
} from './types'
import builtin from '../../../src-tauri/resources/plugins/dsvideo-plugin/skills/ecom-h3-video/templates/bedroom-ugc-product-presenter-15s.json'
import '../images/ImageStudio.css'
import '../images/studioLayout.css'
import './VideoStudio.css'
import { VideoMediaOptions } from './VideoMediaOptions'
import { readVideoDrafts, writeVideoDraft, type VideoEntry } from './videoDrafts'
import { ChatMarkdown } from '../ChatMarkdown'
import { useSharedDraft } from '../studio/useSharedDraft'
import {
  applyVideoStudioDrop,
  videoDropZoneFromPoint,
  type VideoDropZone,
} from './videoDrop'
import { useTaskLibrary } from '../studio/useTaskLibrary'
import { VideoTaskPanel } from './VideoTaskPanel'
import { useVideoTaskProgress } from './useVideoTaskProgress'

const preview: VideoBootstrap = {
  tasks: [],
  templates: [{ ...builtin, kind: 'generation' }],
  config: {},
  root: '',
  configPath: '',
  dependencies: { python: '', comfy: false, node: false, ffmpeg: false },
}
const routeNames = {
  comfy: 'ComfyUI',
  minimax: 'MiniMax H3',
  grok: 'Grok Video',
}

function AssetImage({ path, name }: { path: string; name: string }) {
  const [src, setSrc] = useState('')
  useEffect(() => {
    let alive = true
    void api
      .videoStudioImage(path)
      .then((s) => {
        if (alive) setSrc(s)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [path])
  return src ? <img src={src} alt={name} /> : <span>{name}</span>
}

export default function VideoStudio() {
  const native = isTauriRuntime()
  const library = useTaskLibrary('video', native)
  const [initial] = useState(() => readVideoDrafts().creation)
  const [entry, setEntry] = useState<VideoEntry>('creation')
  const [draftSaved, setDraftSaved] = useState(true)
  const [editingScript, setEditingScript] = useState(false)
  const [data, setData] = useState<VideoBootstrap>(preview)
  const [runtimeCheck, setRuntimeCheck] = useState<
    'pending' | 'checking' | 'ready' | 'failed'
  >('pending')
  const [runtimeError, setRuntimeError] = useState('')
  const [view, setView] = useState<
    VideoEntry | 'templates' | 'settings' | 'tasks'
  >('creation')
  const [brief, setBrief] = useState<VideoBrief>(() => initial?.brief || newVideoBrief())
  const [task, setTask] = useState<VideoTask | undefined>(initial?.task)
  const [script, setScript] = useState(initial?.script || '')
  const [dirty, setDirty] = useState(initial?.dirty || false)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [step, setStep] = useState(initial?.step || 0)
  const [templateName, setTemplateName] = useState('')
  const [provider, setProvider] = useState('comfy')
  const [base, setBase] = useState('http://127.0.0.1:8188')
  const [key, setKey] = useState('')
  const [configDirty, setConfigDirty] = useState(false)
  const [configConflict, setConfigConflict] = useState(false)
  const configVersion = useRef('')
  const [model, setModel] = useState('grok-imagine-video-1.5')
  const [video, setVideo] = useState('')
  const [recoveryId, setRecoveryId] = useState('')
  const [dropActive, setDropActive] = useState(false)
  const [dropTarget, setDropTarget] = useState<VideoDropZone | null>(null)
  const isAnalysis = view === 'analysis' || view === 'remake'
  const locked =
    !!task &&
    ['submitting', 'running', 'succeeded', 'uncertain'].includes(task.status)
  const dropReadyRef = useRef({ accept: false, busy: false })
  const briefRef = useRef(brief)
  const viewRef = useRef(view)
  briefRef.current = brief
  viewRef.current = view
  dropReadyRef.current = {
    accept: native && (view === 'creation' || isAnalysis) && step === 0,
    busy: !!busy || locked,
  }

  const shared = useSharedDraft('video', entry, native,
    { brief, task, script, step, dirty },
    (draft) => {
      setBrief(draft.brief); setTask(draft.task); setScript(draft.script)
      setStep(draft.step); setDirty(draft.dirty)
    }, !!busy)
  const syncCurrent = useRef({ task, dirty, busy })
  syncCurrent.current = { task, dirty, busy }
  const progressError = useVideoTaskProgress(data.tasks, native && view === 'tasks' && !busy, updated => {
    setData(d => ({ ...d, tasks: d.tasks.map(t => t.id === updated.id && t.revision <= updated.revision ? updated : t) }))
  })

  const refresh = useCallback(async (silent = false) => {
    if (!native) return
    if (!silent) setRuntimeCheck('checking')
    setRuntimeError('')
    try {
      const next = await api.videoStudioBootstrap()
      setData(next)
      const current = syncCurrent.current
      const updated = next.tasks.find(t => t.id === current.task?.id)
      if (updated && JSON.stringify(updated) !== JSON.stringify(current.task) && !current.busy) {
        if (!current.dirty) {
          setTask(updated); setBrief(updated.brief); setScript(updated.script)
          setStep(updated.prompt ? 2 : updated.script || updated.concepts?.length ? 1 : 0)
        } else setError('此任务已在聊天中更新。本地编辑已保留，请重新打开最新任务后继续。')
      }
      setRuntimeCheck('ready')
    } catch (e) {
      setRuntimeCheck('failed')
      setRuntimeError(String(e))
      throw e
    }
  }, [native])
  useEffect(() => {
    void refresh().catch(() => {})
  }, [refresh])
  useEffect(() => {
    if (!error) return
    const timer = window.setTimeout(() => setError(''), 5000)
    return () => window.clearTimeout(timer)
  }, [error])
  useEffect(() => {
    setVideo('')
    let alive = true
    if (native && task?.output)
      void api
        .videoStudioPreview(task.id)
        .then((url) => {
          if (alive) setVideo(url)
        })
        .catch(() => {})
    return () => {
      alive = false
    }
  }, [native, task?.id, task?.output])
  // Refresh shared chat-created templates when returning to this window.
  useEffect(() => {
    const focus = () => {
      void refresh(true).catch(() => {})
    }
    const timer = window.setInterval(() => { if (document.visibilityState !== 'hidden') focus() }, 3000)
    window.addEventListener('focus', focus)
    return () => { window.clearInterval(timer); window.removeEventListener('focus', focus) }
  }, [refresh])

  useEffect(() => {
    const version = JSON.stringify([provider, data.config[provider]])
    if (configDirty) {
      if (configVersion.current !== version) setConfigConflict(true)
      return
    }
    configVersion.current = version
    setBase(data.config[provider]?.base_url || ({ comfy: 'http://127.0.0.1:8188', minimax: 'https://api.minimaxi.com', grok: 'https://api.x.ai' } as Record<string, string>)[provider])
    setModel(data.config[provider]?.model || 'grok-imagine-video-1.5')
    setConfigConflict(false)
  }, [provider, data.config, configDirty])

  const uncheckedRuntime = runtimeCheck === 'checking'
    ? '检测中…'
    : runtimeCheck === 'failed' ? '检测失败' : '未检测'
  const dependencyStatus = (available?: boolean) => runtimeCheck === 'ready'
    ? available === undefined ? '未检测' : available ? '已就绪' : '内置文件缺失'
    : uncheckedRuntime
  const runtimeMissing = runtimeCheck === 'ready' &&
    [data.dependencies.comfy, data.dependencies.analyzer, data.dependencies.node,
      data.dependencies.ffmpeg, data.dependencies.bundled].some(value => value === false)

  useEffect(() => {
    if (view !== 'creation' && view !== 'analysis' && view !== 'remake') return
    setDraftSaved(writeVideoDraft(entry, { brief, task, script, step, dirty }))
  }, [entry, view, brief, task, script, step, dirty])

  function navigate(next: VideoEntry) {
    writeVideoDraft(entry, { brief, task, script, step, dirty })
    const draft = readVideoDrafts()[next]
    setEntry(next)
    setView(next)
    setBrief(draft?.brief || newVideoBrief(next === 'creation' ? 'creation' : 'analysis'))
    setTask(draft?.task)
    setScript(draft?.script || '')
    setDirty(draft?.dirty || false)
    setStep(draft?.step || 0)
    setEditingScript(false)
  }

  function useAnalysis() {
    writeVideoDraft(entry, { brief, task, script, step, dirty })
    const images = brief.images
    const reference: VideoTemplate = { id: task?.id || 'reference-draft', name: brief.name || '本次参考视频', kind: 'reference', script }
    fresh('creation', reference)
    setBrief(b => ({ ...b, images, request: '沿用已确认参考的镜头结构、动作和节奏，适配本次商品；不新增无关剧情。' }))
  }

  function accept(t: VideoTask) {
    setTask(t)
    setBrief(t.brief)
    setScript(t.script)
    setDirty(false)
    setData((d) => ({
      ...d,
      tasks: [t, ...d.tasks.filter((x) => x.id !== t.id)],
    }))
  }
  function change(values: Partial<VideoBrief>) {
    setBrief((b) => ({ ...b, ...values }))
    setDirty(true)
  }
  const openTask = (t: VideoTask) => void guarded('打开任务…', async () => {
    if (dirty && (brief.request.trim() || brief.images.length || script.trim())) await saved()
    const latest = await api.videoStudioTask('get', { id: t.id })
    accept(latest)
    setEntry(latest.brief.mode)
    setView(latest.brief.mode)
    setEditingScript(false)
    setStep(latest.prompt || latest.output ? 2 : latest.script || latest.concepts?.length ? 1 : 0)
  })
  function fresh(mode: VideoEntry, template?: VideoTemplate) {
    setTask(undefined)
    setBrief({
      ...newVideoBrief(mode === 'creation' ? 'creation' : 'analysis'),
      template,
      ...(template?.spec?.duration_seconds
        ? { duration: template.spec.duration_seconds }
        : {}),
      ...(template?.spec?.aspect_ratio
        ? { ratio: template.spec.aspect_ratio }
        : {}),
    })
    setScript('')
    setDirty(false)
    setEntry(mode)
    setView(mode)
    setEditingScript(false)
    setStep(0)
    setError('')
    setTemplateName('')
  }
  async function guarded(label: string, fn: () => Promise<void>) {
    if (busy) return
    setBusy(label)
    setError('')
    try {
      await fn()
    } catch (e) {
      setError(String(e))
    } finally {
      setBusy('')
    }
  }
  async function saved(patch?: Partial<VideoBrief>) {
    const nextBrief = { ...brief, ...patch, name: brief.name || brief.request.trim().slice(0, 24) || (brief.mode === 'analysis' ? '视频拆解' : '视频创作') }
    let t = task
    if (!t) t = await api.videoStudioTask('create', { brief: nextBrief })
    if (dirty || patch)
      t = await api.videoStudioTask('save', {
        id: t.id,
        revision: t.revision,
        brief: nextBrief,
        script,
      })
    accept(t)
    return t
  }
  async function run(action: string, patch?: Partial<VideoBrief>) {
    await guarded(
      (
        {
          plan: '导演正在编写剧本…',
          analyze: '正在拆解参考视频…',
          approve: '正在转换已确认剧本…',
          submit: '正在提交生成…',
          quote: '正在查询报价…',
          poll: '正在查询远程任务…',
        } as Record<string, string>
      )[action] || '保存中…',
      async () => {
        let t =
          (action === 'get' || action === 'poll') && task
            ? await api.videoStudioTask('get', { id: task.id })
            : await saved(patch)
        if (action === 'poll' && (t.status === 'succeeded' || !t.remote?.id)) { accept(t); return }
        if (action !== 'save' && action !== 'get') {
          t = await api.videoStudioTask(action, {
            id: t.id,
            revision: t.revision,
            confirmSpend: action === 'submit',
          })
          accept(t)
          if (action === 'approve') {
            t = await api.videoStudioTask('prepare', {
              id: t.id,
              revision: t.revision,
            })
            accept(t)
            setStep(2)
            try {
              t = await api.videoStudioTask('quote', { id: t.id, revision: t.revision })
            } catch {
              // Pricing is optional; the prepared prompt remains ready to submit.
            }
          }
        }
        accept(t)
        if (action === 'plan' || action === 'analyze') setStep(1)
        if (action === 'approve' || action === 'submit') setStep(2)
      },
    )
  }
  const pollRef = useRef(() => { void run('poll') })
  pollRef.current = () => { void run('poll') }
  useEffect(() => {
    if (!native || busy || view === 'tasks' || task?.status !== 'running' || !task.remote?.id) return
    const timer = window.setTimeout(() => pollRef.current(), 8000)
    return () => window.clearTimeout(timer)
  }, [native, busy, view, task?.id, task?.revision, task?.status, task?.remote?.id])

  async function pickImages() {
    await guarded('选择素材…', async () => {
      const result = await open({
        multiple: true,
        filters: [
          { name: '商品图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] },
        ],
      })
      if (result)
        change({
          images: [
            ...new Set([
              ...brief.images,
              ...(Array.isArray(result) ? result : [result]),
            ]),
          ],
        })
    })
  }
  const dropTargetRef = useRef<VideoDropZone | null>(null)
  const markDropTarget = (zone: VideoDropZone | null) => {
    dropTargetRef.current = zone
    setDropTarget(zone)
  }
  const dropTargetFromPosition = (position?: { x: number; y: number }) => {
    if (!position || !Number.isFinite(position.x) || !Number.isFinite(position.y)) {
      return dropTargetRef.current
    }
    const scale = window.devicePixelRatio || 1
    return videoDropZoneFromPoint(position.x / scale, position.y / scale) || dropTargetRef.current
  }
  const importDropped = (paths: string[], zone: VideoDropZone | null = dropTargetRef.current) => {
    if (dropReadyRef.current.busy || !paths.length) return
    const page = viewRef.current
    const result = applyVideoStudioDrop(page, briefRef.current, paths, zone)
    if ('error' in result) {
      setError(result.error)
      return
    }
    change(result)
  }
  const importDroppedRef = useRef(importDropped)
  importDroppedRef.current = importDropped
  useEffect(() => {
    if (!native) return
    let cancelled = false
    let unlisten: (() => void) | undefined
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (cancelled) return
        const payload = event.payload
        if (!dropReadyRef.current.accept) {
          if (payload.type === 'leave' || payload.type === 'drop') {
            setDropActive(false)
            markDropTarget(null)
          }
          return
        }
        if (payload.type === 'enter' || payload.type === 'over') {
          if (!dropReadyRef.current.busy) {
            markDropTarget(dropTargetFromPosition(payload.position))
            setDropActive(true)
          }
          return
        }
        if (payload.type === 'leave') {
          setDropActive(false)
          markDropTarget(null)
          return
        }
        if (payload.type === 'drop') {
          const zone = dropTargetFromPosition(payload.position)
          setDropActive(false)
          markDropTarget(null)
          importDroppedRef.current(payload.paths, zone)
        }
      })
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch((err) => console.error('Video studio drag-drop listen failed:', err))
    return () => {
      cancelled = true
      setDropActive(false)
      markDropTarget(null)
      unlisten?.()
    }
  }, [native])
  const keepOsDrop = (event: DragEvent, zone?: VideoDropZone) => {
    event.preventDefault()
    event.stopPropagation()
    if (zone) markDropTarget(zone)
  }
  const controlsDisabled = !!busy || locked
  const route = brief.route
  const resolutions =
    route === 'grok'
      ? brief.inputMode === 'reference' ||
        (brief.inputMode !== 'image' &&
          (brief.images.length > 1 || !!brief.voiceIds?.length))
        ? ['480p', '720p']
        : ['480p', '720p', '1080p']
      : route === 'minimax'
        ? ['768P', '2K']
        : route === 'comfy'
          ? ['0.5', '1']
          : []

  return (
    <div className={`kv image-studio video-studio${isAnalysis ? " vs-analysis" : ""}${view === "remake" ? " vs-remake" : ""}`}>
      {error && (
        <div role="alert" className="vs-toast">
          <span>{error}</span>
          <IconButton label="关闭提示" onClick={() => setError('')}>
            <X size={14} />
          </IconButton>
        </div>
      )}
      {!native && (
        <div className="is-preview-note">
          浏览器布局预览 · 请在桌面应用中使用 Agent、素材和生成服务。
        </div>
      )}
      <div className="is-shell">
        <aside className="is-rail">
          <div className="is-rail-label">视频工作台</div>
          <nav>
            <button
              className={view === 'creation' ? 'active' : ''}
              disabled={!!busy}
              onClick={() => {
                navigate('creation')
              }}
            >
              <Clapperboard size={17} />
              <span>视频创作</span>
            </button>
            <button className={view === 'remake' ? 'active' : ''} disabled={!!busy} onClick={() => navigate('remake')}>
              <Layers size={17} /><span>参考仿拍</span>
            </button>
            <button
              className={view === 'analysis' ? 'active' : ''}
              disabled={!!busy}
              onClick={() => {
                navigate('analysis')
              }}
            >
              <ScanSearch size={17} />
              <span>视频拆解</span>
            </button>
            <button
              className={view === 'templates' ? 'active' : ''}
              disabled={!!busy}
              onClick={() => {
                setView('templates')
                void guarded('刷新模板…', refresh)
              }}
            >
              <Layers size={17} />
              <span>模板库</span>
              <small>{data.templates.length}</small>
            </button>
            <button className={view === 'tasks' ? 'active' : ''} aria-current={view === 'tasks' ? 'page' : undefined} disabled={!!busy} onClick={() => setView('tasks')}>
              <History size={17} /><span>任务</span><small>{data.tasks.filter(t => ['running', 'submitting', 'uncertain'].includes(t.status) || !library.organization[t.id]?.archived).length}</small>
            </button>
          </nav>
          <div className="is-rail-history custom-scrollbar">
            <div className="is-rail-label">最近任务</div>
            {!data.tasks.length && (
              <p className="vs-muted">剧本、成片和远程任务会保存在这里。</p>
            )}
            {data.tasks.filter(t => ['running', 'submitting', 'uncertain'].includes(t.status) || !library.organization[t.id]?.archived).sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 6).map((t) => (
              <button
                key={t.id}
                disabled={!!busy}
                className={t.id === task?.id ? 'active' : ''}
                onClick={() => openTask(t)}
              >
                <span className={`is-history-dot ${t.status}`} />
                <span>{t.brief.name || '未命名视频'}</span>
              </button>
            ))}
          </div>
          <div className="is-rail-foot">
            <Button
              size="sm"
              variant="ghost"
              disabled={!!busy}
              onClick={() => setView('settings')}
            >
              <Settings2 size={15} />
              视频设置
            </Button>
          </div>
        </aside>
        <main className="is-main custom-scrollbar">
          {shared.message && (
            <div role="status">
              {shared.message}
              {shared.hasConflict && <>
                <Button onClick={shared.reload}>载入共享版本</Button>
                <Button onClick={shared.keep}>保留本地版本</Button>
              </>}
            </div>
          )}
          {busy && (
            <div role="status" className="vs-notice">
              <RefreshCw size={14} className="vs-spin" />
              {busy}
            </div>
          )}
          {view === 'tasks' ? (
            <>
            {progressError && <p role="status" className="tl-message">{progressError}</p>}
            {runtimeCheck === 'failed' && <p role="alert" className="tl-message tl-error">任务读取失败：{runtimeError}。请刷新任务重试。</p>}
            <VideoTaskPanel tasks={data.tasks} library={library} loading={runtimeCheck === 'checking' || runtimeCheck === 'pending'} disabled={!!busy}
              currentId={task?.id} onOpen={openTask} onRefresh={() => refresh(true)} onNew={() => void guarded('新建视频…', async () => {
                if (dirty && (brief.request.trim() || brief.images.length || script.trim())) await saved()
                fresh('creation')
              })} />
            </>
          ) : view === 'settings' ? (
            <>
              <div className="vs-heading">
                <h2>视频设置</h2>
                <span className="vs-muted">聊天与视频页面共用</span>
              </div>
              <div className="vs-settings-layout">
                <section className="vs-panel vs-settings">
                  <Field label="配置路线">
                    <StudioSelect
                      value={provider}
                      onChange={(e) => {
                        const p = e.target.value
                        setProvider(p)
                        setConfigDirty(false)
                        setKey('')
                        setBase(
                          data.config[p]?.base_url ||
                            (
                              {
                                comfy: 'http://127.0.0.1:8188',
                                minimax: 'https://api.minimaxi.com',
                                grok: 'https://api.x.ai',
                              } as Record<string, string>
                            )[p],
                        )
                        setModel(
                          data.config[p]?.model || 'grok-imagine-video-1.5',
                        )
                      }}
                    >
                      {Object.entries(routeNames).map(([v, n]) => (
                        <option key={v} value={v}>
                          {n}
                        </option>
                      ))}
                    </StudioSelect>
                  </Field>
                  <Field label="服务地址">
                    <input
                      className="kv-input"
                      value={base}
                      onChange={(e) => { setConfigDirty(true); setBase(e.target.value) }}
                    />
                  </Field>
                  {provider !== 'comfy' && (
                    <Field
                      label="API Key"
                      hint={
                        data.config[provider]?.ready
                          ? '已配置。留空保留现有密钥。'
                          : '密钥仅保存在本机供应商配置中。'
                      }
                    >
                      <input
                        className="kv-input"
                        type="password"
                        autoComplete="new-password"
                        value={key}
                        onChange={(e) => { setConfigDirty(true); setKey(e.target.value) }}
                      />
                    </Field>
                  )}
                  {provider === 'grok' && (
                    <Field label="模型">
                      <input
                        className="kv-input"
                        value={model}
                        onChange={(e) => { setConfigDirty(true); setModel(e.target.value) }}
                      />
                    </Field>
                  )}
                  <div className="vs-actions">
                    {configConflict && <span role="status">配置已在其他入口修改，请载入最新配置后再编辑。<Button onClick={() => { setConfigDirty(false); setKey('') }}>载入最新配置</Button></span>}
                    <Button
                      variant="primary"
                      disabled={!native || !!busy || configConflict}
                      onClick={() =>
                        void guarded('保存配置…', async () => {
                          const config = await api.videoStudioConfig({
                            name: provider,
                            base_url: base,
                            api_key: key,
                            model,
                          })
                          setData((d) => ({ ...d, config }))
                          setConfigDirty(false)
                          setKey('')
                        })
                      }
                    >
                      保存配置
                    </Button>
                  </div>
                </section>
                <section className="vs-panel">
                  <h3>内置运行环境</h3>
                  <p className="vs-muted">
                    导演使用应用当前聊天模型，处理图片需模型支持视觉。
                  </p>
                  <dl className="vs-specs">
                    <dt>Python</dt>
                    <dd>{runtimeCheck === 'ready' ? data.dependencies.python || '未检测' : uncheckedRuntime}</dd>
                    <dt>Comfy MCP</dt>
                    <dd>{dependencyStatus(data.dependencies.comfy)}</dd>
                    <dt>视频分析 MCP</dt>
                    <dd>{dependencyStatus(data.dependencies.analyzer)}</dd>
                    <dt>Node.js</dt>
                    <dd>
                      {dependencyStatus(data.dependencies.node)}
                    </dd>
                    <dt>FFmpeg</dt>
                    <dd>{dependencyStatus(data.dependencies.ffmpeg)}</dd>
                  </dl>
                  <p className="vs-muted">
                    MCP 服务、Python、Node.js 和视频处理依赖随应用提供。
                    ComfyUI 路线需连接已部署 H3 工作流节点的服务端。
                  </p>
                  <div className="vs-actions">
                    <Button
                      size="sm"
                      disabled={!native || !!busy || runtimeCheck === 'checking'}
                      onClick={() => void guarded('检查环境…', refresh)}
                    >
                      重新检查
                    </Button>
                  </div>
                  {runtimeCheck === 'failed' && (
                    <p className="vs-muted">
                      无法读取内置运行环境状态，请重新检查。错误：{runtimeError}
                    </p>
                  )}
                  {runtimeMissing && (
                    <p className="vs-muted">内置运行环境不完整，请重新安装 Dsivio。</p>
                  )}
                  <p className="vs-path">{data.configPath}</p>
                </section>
              </div>
            </>
          ) : view === 'templates' ? (
            <>
              <div className="vs-heading">
                <h2>
                  模板库 <small>{data.templates.length}</small>
                </h2>
                <div className="vs-actions">
                  <IconButton
                    label="刷新共享模板"
                    disabled={!native || !!busy}
                    onClick={() => void guarded('刷新模板…', refresh)}
                  >
                    <RefreshCw size={15} />
                  </IconButton>
                  <Button
                    size="sm"
                    disabled={!native || !!busy}
                    onClick={() =>
                      void guarded('导入模板…', async () => {
                        const path = await open({
                          filters: [{ name: '视频模板', extensions: ['json'] }],
                        })
                        if (typeof path === 'string') {
                          await api.videoStudioTemplate('template_import', {
                            path,
                            kind: 'reference',
                          })
                          await refresh()
                        }
                      })
                    }
                  >
                    <FolderOpen size={14} />
                    导入参考模板
                  </Button>
                </div>
              </div>
              <p className="vs-muted">
                成片验证模板与参考拆解模板分别标注。聊天保存的模板也会出现在这里。
              </p>
              <div className="vs-template-grid">
                {data.templates.map((t) => (
                  <article className="vs-template" key={t.id}>
                    <div className="vs-template-preview">
                      <Film size={22} />
                      <span>
                        {t.spec?.duration_seconds
                          ? `${t.spec.duration_seconds} 秒`
                          : '参考剧本'}{' '}
                        · {t.spec?.aspect_ratio || '自定义'}
                      </span>
                      {t.shots?.slice(0, 3).map((s, i) => (
                        <div key={i}>
                          <small>{s.time}</small>
                          <span>{s.purpose}</span>
                        </div>
                      ))}
                    </div>
                    <div className="vs-template-body">
                      <small className="vs-muted">
                        {t.kind === 'reference'
                          ? '参考拆解 · 未验证成片'
                          : '成片验证模板'}
                      </small>
                      <h3>{t.name}</h3>
                      <details>
                        <summary>查看剧本</summary>
                        <pre className="custom-scrollbar">
                          {t.script ||
                            t.shots
                              ?.map(
                                (s) =>
                                  `${s.time} ${s.purpose}\n${s.action}\n${s.camera}`,
                              )
                              .join('\n\n')}
                        </pre>
                      </details>
                      <Button
                        size="sm"
                        variant="primary"
                        onClick={() => fresh('creation', t)}
                      >
                        使用模板
                      </Button>
                    </div>
                  </article>
                ))}
              </div>
            </>
          ) : (
            <>
              <div className="vs-heading">
                <h2>{view === 'remake' ? '参考仿拍' : isAnalysis ? '视频拆解' : '视频创作'}</h2>
              </div>
              <div className="is-work-toolbar">
                <div
                  className="is-stage-tabs"
                  role="tablist"
                  aria-label="视频制作步骤"
                >
                  {(isAnalysis
                    ? ['参考素材', '拆解结果']
                    : ['素材与要求', '拍摄方案', '生成与成片']
                  ).map((name, i) => (
                    <button
                      role="tab"
                      aria-selected={step === i}
                      key={name}
                      onClick={() => setStep(i)}
                    >
                      <span>{i + 1}</span>
                      {name}
                    </button>
                  ))}
                </div>
                <div className="vs-actions is-task-actions">
                  <span className="vs-muted">
                    {task ? videoStatus[task.status] || task.status : '新任务'}
                    {dirty ? ' · 未保存' : ''}
                  </span>
                  <Button
                    size="sm"
                    disabled={!!busy}
                    onClick={() => fresh(entry)}
                  >
                    <Plus size={14} />
                    新建
                  </Button>
                  <span className="vs-muted">{draftSaved ? '草稿已保存在本机' : '草稿保存失败，请检查本机空间'}</span>
                  {task && (
                    <IconButton
                      label="重新读取任务"
                      disabled={!!busy}
                      onClick={() => void run('get')}
                    >
                      <RefreshCw size={14} />
                    </IconButton>
                  )}
                </div>
              </div>
              {step === 0 ? (
                <div className="vs-layout">
                  <div className="vs-editor-column">
                    <section
                      className="vs-panel vs-media"
                      onDragEnter={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                      onDragOver={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                      onDrop={(event) => keepOsDrop(event, isAnalysis ? 'source' : 'images')}
                    >
                      <div className="vs-media-head">
                        <h3>{isAnalysis ? '参考视频' : '商品素材'}</h3>
                        <small>
                          {isAnalysis
                            ? '单条视频 · 拆解不会提交生成'
                            : '也可以只写要求，不放图'}
                        </small>
                      </div>
                      {isAnalysis ? (
                        <>
                          <div
                            className={`vs-drop${brief.source ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'images' ? ' is-drop-active' : ''}`}
                            data-video-drop="source"
                            aria-label="参考视频投放区"
                            onDragEnter={(event) => keepOsDrop(event, 'source')}
                            onDragOver={(event) => keepOsDrop(event, 'source')}
                            onDrop={(event) => keepOsDrop(event, 'source')}
                          >
                            {brief.source ? (
                              <div className="vs-drop-file">
                                <span className="vs-drop-mark">
                                  <Film size={20} strokeWidth={1.6} />
                                </span>
                                <div>
                                  <strong>
                                    {brief.source.split(/[\\/]/).pop() || brief.source}
                                  </strong>
                                  <span title={brief.source}>
                                    {dropActive && dropTarget !== 'images'
                                      ? '松开即可替换'
                                      : '还可以把视频继续拖进来替换'}
                                  </span>
                                </div>
                                <IconButton
                                  label="移除参考视频"
                                  disabled={controlsDisabled}
                                  onClick={() => change({ source: '' })}
                                >
                                  <X size={14} />
                                </IconButton>
                              </div>
                            ) : (
                              <div className="vs-drop-empty">
                                <span className="vs-drop-mark">
                                  <Film size={22} strokeWidth={1.5} />
                                </span>
                                <strong>
                                  {dropActive && dropTarget !== 'images'
                                    ? '松开即可导入'
                                    : '把参考视频拖到这里'}
                                </strong>
                                <span>MP4 / MOV / WebM</span>
                              </div>
                            )}
                          </div>
                          <div className="vs-drop-bar">
                            <input
                              className="kv-input"
                              aria-label="视频链接或本地路径"
                              disabled={controlsDisabled}
                              placeholder="粘贴视频链接，或把视频拖到这里"
                              value={brief.source}
                              onChange={(e) =>
                                change({ source: e.target.value })
                              }
                            />
                            <Button
                              size="sm"
                              disabled={!native || controlsDisabled}
                              onClick={() =>
                                void guarded('选择视频…', async () => {
                                  const path = await open({
                                    filters: [
                                      {
                                        name: '视频',
                                        extensions: ['mp4', 'mov', 'webm'],
                                      },
                                    ],
                                  })
                                  if (typeof path === 'string')
                                    change({ source: path })
                                })
                              }
                            >
                              <FolderOpen size={14} />
                              {brief.source ? '更换本地视频' : '选择本地视频'}
                            </Button>
                          </div>
                        </>
                      ) : (
                        <>
                          <div
                            className={`vs-drop${brief.images.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget !== 'referenceVideos' && dropTarget !== 'referenceAudios' ? ' is-drop-active' : ''}`}
                            data-video-drop="images"
                            aria-label="商品素材投放区"
                            onDragEnter={(event) => keepOsDrop(event, 'images')}
                            onDragOver={(event) => keepOsDrop(event, 'images')}
                            onDrop={(event) => keepOsDrop(event, 'images')}
                          >
                            {brief.images.length ? (
                              <div className="vs-assets">
                                {brief.images.map((path) => (
                                  <div key={path}>
                                    <AssetImage
                                      path={path}
                                      name={path.split(/[\\/]/).pop() || '参考图'}
                                    />
                                    <IconButton
                                      label="移除参考图"
                                      disabled={controlsDisabled}
                                      onClick={() =>
                                        change({
                                          images: brief.images.filter(
                                            (p) => p !== path,
                                          ),
                                        })
                                      }
                                    >
                                      <X size={13} />
                                    </IconButton>
                                  </div>
                                ))}
                              </div>
                            ) : (
                              <div className="vs-drop-empty">
                                <span className="vs-drop-mark">
                                  <FileImage size={22} strokeWidth={1.5} />
                                </span>
                                <strong>
                                  {dropActive && dropTarget !== 'referenceVideos' && dropTarget !== 'referenceAudios'
                                    ? '松开即可导入'
                                    : '把商品图片拖到这里'}
                                </strong>
                                <span>PNG / JPG / WebP</span>
                              </div>
                            )}
                          </div>
                          <div className="vs-drop-bar">
                            <small>
                              {dropActive
                                ? '松开即可继续导入'
                                : brief.images.length
                                  ? '还可以把图片继续拖进来'
                                  : '图片用途由系统自动匹配'}
                            </small>
                            <Button
                              size="sm"
                              disabled={!native || controlsDisabled}
                              onClick={() => void pickImages()}
                            >
                              <Plus size={14} />
                              {brief.images.length ? '继续添加参考图' : '选择商品图片'}
                            </Button>
                          </div>
                        </>
                      )}
                    </section>
                    {view === 'remake' && <section className="vs-panel">
                      <h3>我的商品</h3>
                      <div
                        className={`vs-drop${brief.images.length ? ' is-upload-area--filled' : ''}${dropActive && dropTarget === 'images' ? ' is-drop-active' : ''}`}
                        data-video-drop="images"
                        aria-label="商品图片投放区"
                        onDragEnter={(event) => keepOsDrop(event, 'images')}
                        onDragOver={(event) => keepOsDrop(event, 'images')}
                        onDrop={(event) => keepOsDrop(event, 'images')}
                      >
                        {brief.images.length ? (
                          <div className="vs-images">{brief.images.map(path => <div key={path}><AssetImage path={path} name={path.split(/[\\/]/).pop() || '商品'} /><IconButton label="移除商品图片" disabled={controlsDisabled} onClick={() => change({ images: brief.images.filter(p => p !== path) })}><X size={14} /></IconButton></div>)}</div>
                        ) : (
                          <div className="vs-drop-empty">
                            <span className="vs-drop-mark">
                              <FileImage size={22} strokeWidth={1.5} />
                            </span>
                            <strong>{dropActive && dropTarget === 'images' ? '松开即可导入' : '把商品图片拖到这里'}</strong>
                            <span>PNG / JPG / WebP</span>
                          </div>
                        )}
                      </div>
                      <div className="vs-drop-bar">
                        <small>
                          {dropActive && dropTarget === 'images'
                            ? '松开即可继续导入'
                            : brief.images.length
                              ? '还可以把商品图片继续拖进来'
                              : '参考视频定镜头，图片定商品外观'}
                        </small>
                        <Button size="sm" disabled={!native || controlsDisabled} onClick={() => void pickImages()}>
                          <Plus size={14} />
                          {brief.images.length ? '继续添加商品图片' : '添加商品图片'}
                        </Button>
                      </div>
                    </section>}
                    <section className="vs-panel">
                      <Field label={isAnalysis ? '重点分析什么（可选）' : '这次要拍什么'}>
                        <textarea className="kv-textarea vs-request custom-scrollbar" value={brief.request} disabled={controlsDisabled}
                          placeholder={isAnalysis ? '例如：重点看开场、商品展示和镜头节奏。留空则完整拆解。' : '例如：让背包在自然光下缓慢转动，展示正面细节，不要口播。'}
                          onChange={e => change({ request: e.target.value, selectedConcept: undefined })} />
                      </Field>
                      {brief.template && (
                        <div className="vs-notice">
                          已选模板：{brief.template.name}
                          <IconButton
                            label="取消模板"
                            disabled={controlsDisabled}
                            onClick={() => change({ template: undefined })}
                          >
                            <X size={14} />
                          </IconButton>
                        </div>
                      )}
                    </section>
                    {view === 'creation' && (
                      <details className="vs-panel vs-advanced">
                        <summary>声音与高级参考设置</summary>
                        <VideoMediaOptions
                          brief={brief}
                          change={change}
                          disabled={controlsDisabled}
                          native={native}
                          dropActive={dropActive}
                          dropTarget={dropTarget}
                          onDrop={keepOsDrop}
                          onError={setError}
                        />
                      </details>
                    )}
                  </div>
                  <section className="vs-panel vs-options">
                    {!isAnalysis && <Field label="声音">
                      <StudioSelect value={brief.speechMode || 'auto'} disabled={controlsDisabled} onChange={e => change({ speechMode: e.target.value as VideoBrief['speechMode'] })}>
                        <option value="auto">按要求自动设计</option><option value="ambient">环境音与音乐</option><option value="dialogue">口播 / 对白</option><option value="silent">静音</option>
                      </StudioSelect>
                    </Field>}
                    {(isAnalysis || brief.speechMode === 'dialogue') && <Field label={isAnalysis ? "报告语言" : "口播 / 文案语言"}>
                      <StudioSelect
                        disabled={controlsDisabled}
                        value={
                          languages.some(([v]) => v === brief.language)
                            ? brief.language
                            : 'custom'
                        }
                        onChange={(e) => change({ language: e.target.value })}
                      >
                        {languages.map(([v, name]) => (
                          <option key={v} value={v}>
                            {name}
                          </option>
                        ))}
                      </StudioSelect>
                    </Field>}
                    {(!languages.some(([v]) => v === brief.language) ||
                      brief.language === 'custom') && (
                      <Field label="其他语言">
                        <input
                          className="kv-input"
                          disabled={controlsDisabled}
                          value={
                            brief.language === 'custom' ? '' : brief.language
                          }
                          onChange={(e) => change({ language: e.target.value })}
                          placeholder="输入语言名称或地区代码"
                        />
                      </Field>
                    )}
                    {view === 'creation' && (
                      <>
                        <Field label="视频时长（秒）">
                          <input
                            className="kv-input"
                            type="number"
                            min={1}
                            max={15}
                            disabled={controlsDisabled}
                            value={brief.duration}
                            onChange={(e) =>
                              change({ duration: Number(e.target.value) })
                            }
                          />
                        </Field>
                        <Field label="画幅">
                          <StudioSelect
                            disabled={controlsDisabled}
                            value={brief.ratio}
                            onChange={(e) => change({ ratio: e.target.value })}
                          >
                            {(route
                              ? videoRatios[route]
                              : videoRatios.comfy
                            ).map((v) => (
                              <option key={v} value={v}>
                                {v === 'adaptive' ? '自适应' : v}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                        <Field
                          label="生成服务"
                          hint="请选择本次使用的服务，费用会在生成前显示。"
                        >
                          <StudioSelect
                            disabled={controlsDisabled}
                            value={route}
                            onChange={(e) =>
                              change({
                                route: e.target.value as VideoBrief['route'],
                                resolution: '',
                                inputMode: 'auto',
                                ratio: '9:16',
                              })
                            }
                          >
                            <option value="">请选择</option>
                            {Object.entries(routeNames).map(([v, n]) => (
                              <option key={v} value={v}>
                                {n}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                        <Field
                          label={
                            '生成清晰度'
                          }
                        >
                          <StudioSelect
                            disabled={controlsDisabled || !route}
                            value={brief.resolution}
                            onChange={(e) =>
                              change({ resolution: e.target.value })
                            }
                          >
                            <option value="">请选择</option>
                            {resolutions.map((v) => (
                              <option key={v} value={v}>
                                {route === 'comfy' ? (v === '0.5' ? '标准' : '高清') : v}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                      </>
                    )}
                  </section>
                  <div className="vs-actions studio-primary-actions">
                    <Button
                      variant="primary"
                      disabled={
                        !native ||
                        controlsDisabled ||
                        (isAnalysis
                          ? !brief.source.trim() || (view === 'remake' && !brief.images.length)
                          : !brief.request.trim() && !brief.template)
                      }
                      onClick={() =>
                        void run(isAnalysis ? 'analyze' : 'plan')
                      }
                    >
                      <WandSparkles size={15} />
                      {view === 'remake' ? '分析参考并适配商品' : isAnalysis ? '开始拆解' : '帮我设计视频'}
                    </Button>

                  </div>
                </div>
              ) : step === 1 ? (
                <>
                  {!!task?.concepts?.length && !script && <section className="vs-panel vs-concepts">
                    <h3>选一个拍法</h3><p className="vs-muted">选好后，为你展开完整方案。</p>
                    {task.concepts.map((concept, index) => <Button key={concept} disabled={!native || controlsDisabled} onClick={() => void run('plan', { selectedConcept: concept })}><b>0{index + 1}</b><span>{concept}</span><span>选择此拍法 →</span></Button>)}
                  </section>}
                  {(!task?.concepts?.length || script) && <section className="vs-panel">
                    <div className="vs-heading">
                      <h3>{isAnalysis ? '逐镜头拆解' : '拍摄方案'}</h3>
                      <small className="vs-muted">
                        {task?.approved && !dirty
                          ? '已确认此版本'
                          : '可编辑 · 修改后需重新确认'}
                      </small>
                    </div>
                    {script && !editingScript && <div className="vs-script-preview"><ChatMarkdown content={script} /></div>}
                    <Button variant="ghost" size="sm" onClick={() => setEditingScript(!editingScript)}>{editingScript ? '完成编辑' : '编辑全文'}</Button>
                    {(editingScript || !script) && <textarea
                      aria-label="视频方案"
                      className="kv-textarea vs-script custom-scrollbar"
                      disabled={controlsDisabled}
                      value={script}
                      onChange={(e) => {
                        setScript(e.target.value)
                        setDirty(true)
                      }}
                      placeholder="先回到素材与要求设计方案，也可以在这里填写已有剧本。"
                    />}
                    <div className="vs-actions">
                      {view === 'creation' && (
                        <Button
                          variant="primary"
                          disabled={
                            !native ||
                            controlsDisabled ||
                            !script.trim() ||
                            !route ||
                            !brief.resolution
                          }
                          onClick={() => void run('approve')}
                        >
                          确认方案
                        </Button>
                      )}
                    </div>
                    {view === 'creation' && (!route || !brief.resolution) && (
                      <p className="vs-muted">
                        请先在「素材与要求」中选择生成服务和清晰度。
                      </p>
                    )}
                  </section>}
                  {isAnalysis && (
                    <section className="vs-panel">
                      <Button variant="primary" disabled={!script.trim() || !!busy} onClick={useAnalysis}>{view === 'remake' ? '确认结构，进入制作' : '用我的商品仿拍'}</Button>
                      <Field label="保存为参考模板">
                        <input
                          className="kv-input"
                          value={templateName}
                          onChange={(e) => setTemplateName(e.target.value)}
                          placeholder="模板名称"
                        />
                      </Field>
                      <Button
                        disabled={
                          !native ||
                          !!busy ||
                          !script.trim() ||
                          !templateName.trim()
                        }
                        onClick={() =>
                          void guarded('保存参考模板…', async () => {
                            const t = await saved()
                            await api.videoStudioTemplate('template_save', {
                              id: t.id,
                              revision: t.revision,
                              name: templateName,
                            })
                            await refresh()
                            setView('templates')
                          })
                        }
                      >
                        确认拆解并保存模板
                      </Button>
                    </section>
                  )}
                </>
              ) : (
                <>
                  <section className="vs-panel">
                    <h3>生成规格</h3>
                    <p>
                      {route ? routeNames[route] : '未选路线'} ·{' '}
                      {brief.duration} 秒 · {brief.ratio} ·{' '}
                      {brief.resolution || '未选清晰度'}
                    </p>
                    <details className="vs-generation-prompt">
                      <summary>查看转换后的提示词</summary>
                      <pre className="custom-scrollbar">
                        {dirty
                          ? '内容已修改，请重新确认剧本。'
                          : task?.prompt || '请先确认方案。'}
                      </pre>
                    </details>
                    {task?.quote && !dirty && (
                      <div className="vs-quote">
                        <strong>
                          {task.quote.currency && task.quote.estimated_cost?.[brief.resolution] != null
                            ? `官方参考 ${task.quote.estimated_cost[brief.resolution]} ${task.quote.currency}`
                            : route === 'comfy' ? 'ComfyUI 工作流执行' : '以供应商实际计费为准'}
                        </strong>
                        <p>{task.quote.note}</p>
                        {task.quote.balance != null && (
                          <small>
                            余额：
                            {typeof task.quote.balance === 'object'
                              ? JSON.stringify(task.quote.balance)
                              : String(task.quote.balance)}
                          </small>
                        )}
                      </div>
                    )}
                    {task?.submission?.state === 'rejected' && <div role="alert" className="vs-notice">
                      <p>{task.submission.reason}{task.submission.httpStatus ? `（HTTP ${task.submission.httpStatus}）` : ''}</p>
                      <Button onClick={() => setView('settings')}>检查视频设置</Button>
                      <small>剧本、素材和提示词已保留，处理后可直接重试。</small>
                    </div>}
                    {!locked && (
                      <div className="vs-actions">
                        <Button
                          disabled={!native || !!busy || dirty || !task?.prompt}
                          onClick={() => void run('quote')}
                        >
                          刷新报价
                        </Button>
                        <Button
                          variant="primary"
                          disabled={
                            !native ||
                            !!busy ||
                            dirty ||
                            !task?.prompt
                          }
                          onClick={() => void run('submit')}
                        >
                          {task?.submission?.retryable ? '重试生成' : route === 'comfy' ? '开始生成' : '生成视频'}
                        </Button>
                      </div>
                    )}
                    {dirty && (
                      <p className="vs-muted">
                        任务已修改，请重新确认剧本与费用。
                      </p>
                    )}
                  </section>
                  {task?.remote && (
                    <section className="vs-panel">
                      <h3>{videoStatus[task.status]}</h3>
                      {task.remote.id && <p className="vs-path">任务编号：{task.remote.id}</p>}
                      {task.status === 'uncertain' ? <>
                        <p role="alert">{task.submission?.reason || '这次提交没有记录到任务编号或具体接口错误，暂时无法确认服务是否接单。'}</p>
                        {task.submission?.httpStatus && <small>HTTP {task.submission.httpStatus}</small>}
                        <Button onClick={() => setView('settings')}>检查视频设置</Button>
                      </> : task.error && <p role="alert">{task.error}</p>}
                      {task.remote.id && task.status !== 'succeeded' && (
                        <Button
                          disabled={!!busy}
                          onClick={() => void run('poll')}
                        >
                          <RefreshCw size={14} />
                          查询进度 / 恢复结果
                        </Button>
                      )}
                      {task.remote.id && <p className="vs-muted">已保存任务编号，重新打开后可继续查询。</p>}
                    </section>
                  )}
                  {task &&
                    ['uncertain', 'submitting'].includes(task.status) && (
                      <details className="vs-panel">
                        <summary>已有任务编号？恢复查询</summary>
                        <Field
                          label="补录远程任务编号"
                          hint="仅当你已经拿到该任务编号时使用。"
                        >
                          <input
                            className="kv-input"
                            value={recoveryId}
                            onChange={(e) => setRecoveryId(e.target.value)}
                          />
                        </Field>
                        <Button
                          disabled={!!busy || !recoveryId.trim()}
                          onClick={() =>
                            void guarded('恢复任务…', async () => {
                              accept(
                                await api.videoStudioTask('recover', {
                                  id: task.id,
                                  revision: task.revision,
                                  remoteId: recoveryId,
                                }),
                              )
                            })
                          }
                        >
                          保存编号并恢复任务
                        </Button>
                      </details>
                    )}
                  {task?.output && (
                    <section className="vs-panel">
                      <h3>生成结果</h3>
                      {video && (
                        <video
                          className="vs-video"
                          src={video}
                          controls
                          preload="metadata"
                        />
                      )}
                      <Button
                        onClick={() =>
                          void guarded('打开成片…', async () => {
                            await api.videoStudioOpen(task.id)
                          })
                        }
                      >
                        <FolderOpen size={14} />
                        打开本地成片
                      </Button>
                      <Field label="保存为成片验证模板">
                        <input
                          className="kv-input"
                          placeholder="确认成片效果后，填写模板名称"
                          value={templateName}
                          onChange={(e) => setTemplateName(e.target.value)}
                        />
                      </Field>
                      <Button
                        disabled={!!busy || !templateName.trim()}
                        onClick={() =>
                          void guarded('保存成片模板…', async () => {
                            await api.videoStudioTemplate('template_save', {
                              id: task.id,
                              revision: task.revision,
                              name: templateName,
                              approvedOutput: true,
                            })
                            await refresh()
                            setView('templates')
                          })
                        }
                      >
                        确认成片并保存模板
                      </Button>
                    </section>
                  )}
                </>
              )}
            </>
          )}
        </main>
      </div>
    </div>
  )
}
