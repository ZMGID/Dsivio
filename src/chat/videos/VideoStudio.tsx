import { useStudioNavigation } from '../studio/useStudioNavigation'
import { useChatRouteActive } from '../chatRouteVisibility'
import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import {
  Clapperboard,
  ExternalLink,
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
  videoTaskStatus,
  videoRatios,
  languages,
  type VideoBootstrap,
  type VideoBrief,
  type VideoTask,
  type VideoTemplate,
} from './types'
import builtin from '../../../src-tauri/resources/plugins/dsvideo-plugin/skills/ecom-h3-video/templates/bedroom-ugc-product-presenter-15s.json'
import '../images/imageStudio.css'
import '../images/studioLayout.css'
import './VideoStudio.css'
import { RequirementComposer } from '../images/RequirementComposer'
import { VideoMediaOptions } from './VideoMediaOptions'
import { readVideoDrafts, readVideoTaskDraft, removeVideoTaskDraft, writeVideoDraft, newVideoDraftBrief, rememberVideoAssistant, rememberVideoSettings, preferredVideoResolution, videoResolutions, type VideoEntry } from './videoDrafts'
import { ChatMarkdown } from '../ChatMarkdown'
import { StudioToast } from '../studio/StudioToast'
import { useSharedDraft } from '../studio/useSharedDraft'
import {
  applyVideoStudioDrop,
  videoDropZoneFromPoint,
  type VideoDropZone,
} from './videoDrop'
import { useTaskLibrary } from '../studio/useTaskLibrary'
import { VideoTaskPanel } from './VideoTaskPanel'
import { ExecutionStatus } from '../studio/ExecutionStatus'
import { videoInputIssue, videoSetupIssue } from './videoValidation'
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
  const routeActive = useChatRouteActive()
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
  const [brief, setBrief] = useState<VideoBrief>(() => initial?.brief || newVideoDraftBrief())
  const [task, setTask] = useState<VideoTask | undefined>(initial?.task)
  const [script, setScript] = useState(initial?.script || '')
  const [dirty, setDirty] = useState(initial?.dirty || false)
  const workspaceVersion = useRef(0)
  const [workspaceKey, setWorkspaceKey] = useState(0)
  const [foreground, setForeground] = useState({ key: -1, view: '', label: '', ticket: 0 })
  const foregroundTicket = useRef(0)
  const foregroundBusy = foreground.key === workspaceKey && foreground.view === view ? foreground.label : ''
  const navigation = useStudioNavigation()
  const [refreshingTemplates, setRefreshingTemplates] = useState(false)
  const selectView = (next: typeof view) => { workspaceVersion.current++; navigation.cancel(); setView(next) }
  const [operations, setOperations] = useState<Record<string, string>>({})
  const operationsRef = useRef(new Set<string>())
  const savingOperations = useRef(new Set<string>())
  const busy = foregroundBusy || ((view === 'creation' || view === 'analysis' || view === 'remake') && task ? operations[task.id] || '' : '')
  const [error, setError] = useState('')
  const [step, setStep] = useState(initial?.step || 0)
  const [templateName, setTemplateName] = useState('')
  const [reviseNote, setReviseNote] = useState('')
  const [provider, setProvider] = useState('comfy')
  const [base, setBase] = useState('http://192.168.1.171:8188')
  const [key, setKey] = useState('')
  const [configDirty, setConfigDirty] = useState(false)
  const [configConflict, setConfigConflict] = useState(false)
  const configVersion = useRef('')
  const [model, setModel] = useState('grok-imagine-video-1.5')
  const [video, setVideo] = useState('')
  const [poster, setPoster] = useState('')
  const [previewError, setPreviewError] = useState('')
  const [recoveryId, setRecoveryId] = useState('')
  const [dropActive, setDropActive] = useState(false)
  const [dropTarget, setDropTarget] = useState<VideoDropZone | null>(null)
  const isAnalysis = view === 'analysis' || view === 'remake'
  const generationActive =
    !!task &&
    ['submitting', 'running'].includes(task.status)
  const dropReadyRef = useRef({ accept: false })
  const briefRef = useRef(brief)
  const viewRef = useRef(view)
  briefRef.current = brief
  viewRef.current = view
  dropReadyRef.current = {
    accept: routeActive && native && (view === 'creation' || isAnalysis) && step === 0,
  }

  const shared = useSharedDraft('video', entry, native,
    { brief, task, script, step, dirty },
    (draft) => {
      setBrief(draft.brief); setTask(draft.task); setScript(draft.script)
      setStep(draft.step); setDirty(draft.dirty)
    }, !!busy)
  const syncCurrent = useRef({ task, dirty, busy })
  syncCurrent.current = { task, dirty, busy }
  const progressError = useVideoTaskProgress([
    ...(task ? [task] : []), ...data.tasks.filter(t => t.id !== task?.id),
  ].filter(t => !operations[t.id]), native, updated => {
    setData(d => ({ ...d, tasks: d.tasks.map(t => t.id === updated.id && t.revision <= updated.revision ? updated : t) }))
    const current = syncCurrent.current
    if (current.task?.id === updated.id && updated.revision >= current.task.revision && !current.busy && !current.dirty) {
      setTask(updated)
    }
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
      if (updated && JSON.stringify(updated) !== JSON.stringify(current.task) && !current.busy && !current.dirty) {
        setTask(updated); setBrief(updated.brief); setScript(updated.script)
        setDirty(false)
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
    setPoster('')
    setPreviewError('')
    let alive = true
    if (native && task?.output) {
      // Load the small first frame independently, including for large videos
      // which can only be played with the local player.
      void api.videoStudioPoster(task.id)
        .then(url => { if (alive) setPoster(url) })
        .catch(() => {}) // A thumbnail failure must not block playback.
      void api
        .videoStudioPreview(task.id)
        .then((url) => {
          if (alive) setVideo(url)
        })
        .catch(e => { if (alive) setPreviewError(String(e) || '预览加载失败，请打开本地成片。') })
    }
    return () => {
      alive = false
    }
  }, [native, task?.id, task?.output, task?.status])
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
    setBase(data.config[provider]?.base_url || ({ comfy: 'http://192.168.1.171:8188', minimax: 'https://api.minimaxi.com', grok: 'https://api.x.ai' } as Record<string, string>)[provider])
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
    workspaceVersion.current++
    setWorkspaceKey(key => key + 1)
    navigation.cancel()
    writeVideoDraft(entry, { brief, task, script, step, dirty })
    const draft = readVideoDrafts()[next]
    setEntry(next)
    setView(next)
    setBrief(draft?.brief || newVideoDraftBrief(next === 'creation' ? 'creation' : 'analysis'))
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
  function beginEdit() {
    workspaceVersion.current++
    navigation.cancel()
    // A running generation keeps its receipt; edits become a separate draft.
    if (generationActive || (task && operationsRef.current.has(task.id)) || foregroundBusy) {
      if (task) setData(d => ({ ...d, tasks: [task, ...d.tasks.filter(t => t.id !== task.id)] }))
      setTask(undefined)
      setWorkspaceKey(key => key + 1)
    }
    setDirty(true)
  }
  function change(values: Partial<VideoBrief>) {
    beginEdit()
    const next = { ...brief, ...values }
    if (values.assistantId !== undefined) rememberVideoAssistant(values.assistantId)
    if (values.route !== undefined) {
      rememberVideoSettings(brief)
      next.resolution = preferredVideoResolution(next)
    }
    if (values.route !== undefined || values.resolution !== undefined) rememberVideoSettings(next)
    setBrief(next)
  }
  const openTask = (t: VideoTask) => {
    workspaceVersion.current++
    setWorkspaceKey(key => key + 1)
    void navigation.open(t.id, async current => {
      // Opening another task must not depend on saving the current server task.
      // The local draft is the recovery copy; an implicit save can fail on a stale
      // revision (or mutate an approved task) and make the clicked row appear inert.
      if (!writeVideoDraft(entry, { brief, task, script, step, dirty })) {
        throw new Error('本地草稿保存失败，请先保存当前任务后再切换。')
      }
      const latest = operationsRef.current.has(t.id) ? t : await api.videoStudioTask('get', { id: t.id })
      if (!current()) return
      const draft = readVideoTaskDraft(latest.id)
      accept(latest)
      setEntry(latest.brief.mode)
      setView(latest.brief.mode)
      setEditingScript(false)
      setStep(latest.prompt || latest.output ? 2 : latest.script || latest.concepts?.length ? 1 : 0)
      if (draft?.dirty) {
        setBrief(draft.brief)
        setScript(draft.script)
        setDirty(true)
        setStep(draft.step)
      }
    }, e => setError(String(e)))
  }
  function fresh(mode: VideoEntry, template?: VideoTemplate) {
    workspaceVersion.current++
    setWorkspaceKey(key => key + 1)
    writeVideoDraft(entry, { brief, task, script, step, dirty })
    navigation.cancel()
    if (brief.mode === 'creation') rememberVideoSettings(brief)
    setTask(undefined)
    setBrief({
      ...newVideoDraftBrief(mode === 'creation' ? 'creation' : 'analysis'),
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
    setReviseNote('')
  }
  async function guarded(label: string, fn: (current: () => boolean) => Promise<void>) {
    if (foregroundBusy) return
    navigation.cancel()
    const version = workspaceVersion.current
    const ticket = ++foregroundTicket.current
    setForeground({ key: workspaceKey, view, label, ticket })
    setError('')
    try {
      await fn(() => version === workspaceVersion.current)
    } catch (e) {
      setError(String(e))
    } finally {
      setForeground(current => current.ticket === ticket ? { ...current, label: '' } : current)
    }
  }
  async function saved(patch?: Partial<VideoBrief>, scriptOverride?: string) {
    const version = workspaceVersion.current
    const nextScript = scriptOverride ?? script
    const nextBrief = { ...brief, ...patch, name: brief.name || brief.request.trim().slice(0, 24) || (brief.mode === 'analysis' ? '视频拆解' : '视频创作') }
    let t = task && !['running', 'submitting'].includes(task.status) ? task : undefined
    if (t) {
      try {
        t = await api.videoStudioTask('get', { id: t.id })
      } catch (error) {
        if (!String(error).includes('VIDEO_TASK_NOT_FOUND') || ['submitting', 'running', 'uncertain'].includes(t.status)) throw error
        t = undefined
      }
    }
    if (t && ['running', 'submitting'].includes(t.status)) t = undefined
    const creating = !t
    if (!t) t = await api.videoStudioTask('create', { brief: nextBrief })
    if (dirty || patch || scriptOverride !== undefined || (creating && nextScript.trim()))
      t = await api.videoStudioTask('save', {
        id: t.id,
        revision: t.revision,
        brief: nextBrief,
        script: nextScript,
      })
    if (version === workspaceVersion.current) accept(t)
    else setData(d => ({ ...d, tasks: [t!, ...d.tasks.filter(x => x.id !== t!.id)] }))
    return t
  }
  async function run(action: string, patch?: Partial<VideoBrief>) {
    const saveKey = task?.id || `draft:${workspaceKey}`
    if (busy || savingOperations.current.has(saveKey) || (task && operationsRef.current.has(task.id))) return
    if (action === 'use_prompt' && !brief.request.trim()) { setError('请先填写已有的视频提示词'); return }
    const version = workspaceVersion.current
    if (action === 'plan' && !brief.request.trim() && !brief.template && !script.trim() && !brief.images.length) { setError('请填写拍摄要求或添加商品素材'); return }
    if (action === 'analyze' && !brief.source.trim()) { setError('请先添加参考视频'); return }
    if (['use_prompt', 'approve', 'prepare', 'quote', 'submit'].includes(action)) {
      const issue = videoInputIssue({ ...brief, ...patch }, action === 'analyze', view === 'remake', script) || (action === 'analyze' ? '' : videoSetupIssue(brief, data))
      if (issue) { setError(issue); return }
    }
    let t: VideoTask | undefined
    savingOperations.current.add(saveKey)
    await guarded('正在保存素材与要求…', async () => {
      t = (action === 'get' || action === 'poll') && task
        ? task : await saved(patch, action === 'use_prompt' ? brief.request : ['submit', 'quote'].includes(action) && !script.trim() ? brief.request : undefined)
    })
    savingOperations.current.delete(saveKey)
    if (!t || operationsRef.current.has(t.id)) return
    const id = t.id
    const originEntry = entry
    operationsRef.current.add(id)
    const setBusy = (label: string) => setOperations(all => ({ ...all, [id]: label }))
    const setStepIfCurrent = (next: number) => {
      if (version === workspaceVersion.current && syncCurrent.current.task?.id === id) setStep(next)
    }
    const acceptResult = (result: VideoTask) => {
      setData(d => ({ ...d, tasks: [result, ...d.tasks.filter(x => x.id !== id)] }))
      const nextStep = result.prompt || result.output ? 2 : result.script || result.concepts?.length ? 1 : 0
      // Persist only the draft that still belongs to this operation.
      const draft = readVideoDrafts()[originEntry]
      if (draft?.task?.id === id && (!draft.dirty || (JSON.stringify(draft.brief) === JSON.stringify(t!.brief) && draft.script === t!.script))) {
        writeVideoDraft(originEntry, { brief: result.brief, task: result, script: result.script, step: syncCurrent.current.task?.id === id && version !== workspaceVersion.current ? draft.step : nextStep, dirty: false })
      }
      if (syncCurrent.current.task?.id === id && !syncCurrent.current.dirty) accept(result)
    }
    try {
      setBusy(({ plan: 'AI 正在编写拍摄方案…', revise: 'AI 正在修改拍摄方案…', use_prompt: '正在使用原提示词…', analyze: '正在分析参考视频…', approve: '正在确认剧本…', submit: '正在提交视频生成…', quote: '正在查询报价…', poll: '正在查询生成状态…' } as Record<string, string>)[action] || '正在读取任务…')
      if (action === 'get' || action === 'poll') t = await api.videoStudioTask('get', { id })
      if (action === 'poll' && (t.status === 'succeeded' || !t.remote?.id)) { acceptResult(t); return }
      if (['submit', 'quote'].includes(action) && (!t.approved || !t.prompt)) {
        t = await api.videoStudioTask('approve', { id, revision: t.revision })
        if (t.brief.route !== 'grok') {
          t = await api.videoStudioTask('prompt_result', { id, revision: t.revision, prompt: t.script })
        }
        acceptResult(t)
      }
      if (action !== 'save' && action !== 'get') {
        t = await api.videoStudioTask(action === 'use_prompt' ? 'approve' : action, { id, revision: t.revision, ...(action === 'revise' ? { note: reviseNote.trim() } : { confirmSpend: action === 'submit' }) })
        acceptResult(t)
        if (action === 'approve' || action === 'use_prompt') {
          if (action === 'use_prompt' && t.brief.route !== 'grok') {
            t = await api.videoStudioTask('prompt_result', { id, revision: t.revision, prompt: t.script })
            acceptResult(t)
          } else if (t.brief.route !== 'grok') {
            setBusy('正在准备生成提示词…')
            t = await api.videoStudioTask('prepare', { id, revision: t.revision })
            acceptResult(t)
          }
          setStepIfCurrent(2)
          setBusy('正在查询报价…')
          try { t = await api.videoStudioTask('quote', { id, revision: t.revision }) } catch { /* Pricing is optional. */ }
        }
      }
      acceptResult(t)
      if (action === 'plan' || action === 'analyze' || action === 'revise') setStepIfCurrent(1)
      if (action === 'revise' && version === workspaceVersion.current) { setReviseNote(''); setEditingScript(false) }
      if (action === 'approve' || action === 'use_prompt' || action === 'submit') setStepIfCurrent(2)
    } catch (e) {
      setError(`${t.brief.name || '视频任务'}：${String(e)}`)
    } finally {
      operationsRef.current.delete(id)
      setOperations(all => { const next = { ...all }; delete next[id]; return next })
    }
  }
  async function revisePlan() {
    if (!script.trim() || !reviseNote.trim()) { setError('请填写方案和修改要求'); return }
    await run('revise')
  }
  async function pickImages() {
    await guarded('选择素材…', async current => {
      const result = await open({
        multiple: true,
        filters: [
          { name: '商品图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] },
        ],
      })
      if (result && current())
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
    if (!paths.length) return
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
          markDropTarget(dropTargetFromPosition(payload.position))
          setDropActive(true)
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
  const inputIssue = videoInputIssue(brief, isAnalysis, view === 'remake', script) || (isAnalysis ? '' : videoSetupIssue(brief, data))
  const route = brief.route
  const resolutions = videoResolutions(brief)

  return (
    <div className={`kv image-studio video-studio${isAnalysis ? " vs-analysis" : ""}${view === "remake" ? " vs-remake" : ""}`}>
      <div className="studio-toasts">
        {error && (
          <StudioToast tone="error" onClose={() => setError('')}>
            {error}
          </StudioToast>
        )}
        {shared.message && <StudioToast>{shared.message}</StudioToast>}
      </div>
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
              onClick={() => {
                navigate('creation')
              }}
            >
              <Clapperboard size={17} />
              <span>视频创作</span>
            </button>
            <button className={view === 'remake' ? 'active' : ''}  onClick={() => navigate('remake')}>
              <Layers size={17} /><span>参考仿拍</span>
            </button>
            <button
              className={view === 'analysis' ? 'active' : ''}
              onClick={() => {
                navigate('analysis')
              }}
            >
              <ScanSearch size={17} />
              <span>视频拆解</span>
            </button>
            <button
              className={view === 'templates' ? 'active' : ''}
              onClick={() => {
                selectView('templates')
                void refresh(true).catch(e => setError(String(e)))
              }}
            >
              <Layers size={17} />
              <span>模板库</span>
              <small>{data.templates.length}</small>
            </button>
            <button className={view === 'tasks' ? 'active' : ''} aria-current={view === 'tasks' ? 'page' : undefined}  onClick={() => selectView('tasks')}>
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
                className={t.id === task?.id ? 'active' : ''}
                onClick={() => openTask(t)}
              >
                <span className={`is-history-dot ${operations[t.id] ? 'running' : t.status}`} />
                <span>{t.brief.name || '未命名视频'}{operations[t.id] ? ' · 处理中' : ''}</span>
              </button>
            ))}
          </div>
          <div className="is-rail-foot">
            <Button
              size="sm"
              variant="ghost"
              onClick={() => selectView('settings')}
            >
              <Settings2 size={15} />
              视频设置
            </Button>
          </div>
        </aside>
        <div className="studio-workspace">
        <main className="is-main custom-scrollbar">
          {view === 'tasks' ? (
            <>
            {progressError && <p role="status" className="tl-message">{progressError}</p>}
            {runtimeCheck === 'failed' && <p role="alert" className="tl-message tl-error">任务读取失败：{runtimeError}。请刷新任务重试。</p>}
            <VideoTaskPanel onDeleted={id => {
                removeVideoTaskDraft(id)
                setData(current => ({ ...current, tasks: current.tasks.filter(t => t.id !== id) }))
                for (const [key, draft] of Object.entries(readVideoDrafts())) {
                  if (draft?.task?.id === id) writeVideoDraft(key as VideoEntry, { ...draft, task: undefined, dirty: false })
                }
                if (task?.id === id) { setTask(undefined); setDirty(false) }
              }} activeOperations={operations} tasks={data.tasks} library={library} loading={runtimeCheck === 'checking' || runtimeCheck === 'pending'}
              currentId={task?.id} onOpen={openTask} onRefresh={() => refresh(true)} onNew={() => fresh('creation')} />
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
                                comfy: 'http://192.168.1.171:8188',
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
                    方案使用当前聊天模型；处理图片需模型支持视觉。
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
                      onClick={() => void guarded('检查环境…', () => refresh())}
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
                    <p className="vs-muted">内置依赖不完整，请重新安装 Dsivio 后重新检查。</p>
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
                    disabled={!native || refreshingTemplates}
                    onClick={() => {
                      setRefreshingTemplates(true)
                      void refresh(true).catch(e => setError(String(e))).finally(() => setRefreshingTemplates(false))
                    }}
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
                      onClick={() => { workspaceVersion.current++; navigation.cancel(); setStep(i) }}
                    >
                      <span>{i + 1}</span>
                      {name}
                    </button>
                  ))}
                </div>
                <div className="vs-actions is-task-actions">
                  <span className="vs-muted">
                    {task ? videoTaskStatus(task) : '新任务'}
                    {dirty ? ' · 未保存' : ''}
                  </span>
                  <Button
                    size="sm"
                    onClick={() => fresh(entry)}
                  >
                    <Plus size={14} />
                    新建
                  </Button>
                  <span className="vs-muted">{draftSaved ? '草稿已保存在本机' : '草稿保存失败，请检查本机空间'}</span>
                  {task && (
                    <IconButton
                      label="重新读取任务"
                      onClick={() => openTask(task)}
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
                              placeholder="粘贴视频链接，或把视频拖到这里"
                              value={brief.source}
                              onChange={(e) =>
                                change({ source: e.target.value })
                              }
                            />
                            <Button
                              size="sm"
                              disabled={!native}
                              onClick={() =>
                                void guarded('选择视频…', async current => {
                                  const path = await open({
                                    filters: [
                                      {
                                        name: '视频',
                                        extensions: ['mp4', 'mov', 'webm'],
                                      },
                                    ],
                                  })
                                  if (typeof path === 'string' && current())
                                    change({ source: path })
                                })
                              }
                            >
                              <FolderOpen size={14} />
                              {brief.source ? '更换本地视频' : '选择本地视频'}
                            </Button>
                          </div>
                          <Field label="分析方式">
                            <StudioSelect
                              value={brief.analysisMethod || 'auto'}
                              onChange={(event) =>
                                change({ analysisMethod: event.target.value as VideoBrief['analysisMethod'] })
                              }
                            >
                              <option value="auto">自动：有视频模型则直传，否则使用 MCP</option>
                              <option value="model">视频模型</option>
                              <option value="mcp">video-analyzer MCP</option>
                            </StudioSelect>
                          </Field>
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
                              disabled={!native}
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
                          <div className="vs-images">{brief.images.map(path => <div key={path}><AssetImage path={path} name={path.split(/[\\/]/).pop() || '商品'} /><IconButton label="移除商品图片" onClick={() => change({ images: brief.images.filter(p => p !== path) })}><X size={14} /></IconButton></div>)}</div>
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
                        <Button size="sm" disabled={!native} onClick={() => void pickImages()}>
                          <Plus size={14} />
                          {brief.images.length ? '继续添加商品图片' : '添加商品图片'}
                        </Button>
                      </div>
                    </section>}
                    <section className="vs-panel">
                      {isAnalysis ? <Field label="重点分析什么（可选）"><textarea className="kv-textarea vs-request custom-scrollbar"
                        value={brief.request} onChange={e => change({ request: e.target.value })}
                        placeholder="例如：重点看开场、商品展示和镜头节奏。留空则完整拆解。" /></Field> : <RequirementComposer label="这次要拍什么"
                        value={brief.request}
                        onChange={request => change({ request, selectedConcept: undefined })}
                        placeholder="例如：让背包在自然光下缓慢转动，展示正面细节，不要口播。" />}
                      {brief.template && (
                        <div className="vs-notice">
                          已选模板：{brief.template.name}
                          <IconButton
                            label="取消模板"
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
                        <VideoMediaOptions onError={setError}
                          disabled={false}
                          brief={brief}
                          change={change}
                          native={native}
                          dropActive={dropActive}
                          dropTarget={dropTarget}
                          onDrop={keepOsDrop}

                        />
                      </details>
                    )}
                  </div>
                  <section className="vs-panel vs-options">
                    {!isAnalysis && <Field label="声音">
                      <StudioSelect value={brief.speechMode || 'auto'} onChange={e => change({ speechMode: e.target.value as VideoBrief['speechMode'] })}>
                        <option value="auto">按要求自动设计</option><option value="ambient">环境音与音乐</option><option value="dialogue">口播 / 对白</option><option value="silent">静音</option>
                      </StudioSelect>
                    </Field>}
                    {(isAnalysis || brief.speechMode === 'dialogue') && <Field label={isAnalysis ? "报告语言" : "口播 / 文案语言"}>
                      <StudioSelect
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
                            value={brief.duration}
                            onChange={(e) =>
                              change({ duration: Number(e.target.value) })
                            }
                          />
                        </Field>
                        <Field label="画幅">
                          <StudioSelect
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
                          hint="记住上次选择的服务和清晰度，费用会在生成前显示。"
                        >
                          <StudioSelect
                            value={route}
                            onChange={(e) =>
                              change({
                                route: e.target.value as VideoBrief['route'],
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
                            disabled={!route}
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
                  {inputIssue && <p className="vs-muted" role="status">{inputIssue} <Button size="sm" onClick={() => selectView('settings')}>配置生成服务</Button></p>}
                  <div className="vs-actions studio-primary-actions">
                    {!isAnalysis && <Button
                      disabled={!native || !!busy || !brief.request.trim()}
                      onClick={() => void run('use_prompt')}
                      title="原文作为生成提示词，跳过方案设计和改写，进入生成页"
                    >
                      <Clapperboard size={15} />使用原提示词生成
                    </Button>}
                    <Button
                      variant="primary"
                      disabled={
                        !native ||
                        !!busy ||
                        (isAnalysis
                          ? !brief.source.trim() || (view === 'remake' && !brief.images.length)
                          : !brief.request.trim() && !brief.template && !brief.images.length)
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
                    {task.concepts.map((concept, index) => <Button key={concept} disabled={!native || !!busy} onClick={() => void run('plan', { selectedConcept: concept })}><b>0{index + 1}</b><span>{concept}</span><span>选择此拍法 →</span></Button>)}
                  </section>}
                  {(!task?.concepts?.length || script) && <section className="vs-panel vs-plan-panel">
                    <div className="vs-plan-controls">
                      <div className="vs-plan-toolbar">
                        <div className="vs-plan-title">
                          <h3>{isAnalysis ? '逐镜头拆解' : '拍摄方案'}</h3>
                          <small className="vs-muted">{task?.approved && !dirty ? '已准备生成' : '可直接编辑，生成时使用当前方案'}</small>
                        </div>
                        <div className="vs-plan-buttons">
                          <Button variant="ghost" size="sm" onClick={() => setEditingScript(!editingScript)}>{editingScript ? '完成编辑' : '编辑全文'}</Button>
                          {view === 'creation' && <Button
                          variant="primary" size="sm"
                          onClick={() => { workspaceVersion.current++; navigation.cancel(); setStep(2) }}
                        >去生成</Button>}
                        </div>
                      </div>
                      {view === 'creation' && !!script.trim() && <div className="vs-plan-revision">
                        <textarea
                          aria-label="修改要求"
                          className="kv-textarea custom-scrollbar"
                          rows={2}
                          value={reviseNote}
                          onChange={e => { workspaceVersion.current++; setReviseNote(e.target.value) }}
                          placeholder="想调整哪里？例如：开头改成鞋子特写，结尾不要字幕。"
                        />
                        <div className="vs-plan-revision-footer">
                          <span>说明要改的地方，AI 帮你调整方案</span>
                          <Button size="sm"
                            disabled={!native || !!busy || !reviseNote.trim()}
                            onClick={() => void revisePlan()}
                          ><WandSparkles size={15} />让 AI 修改</Button>
                        </div>
                      </div>}
                      {view === 'creation' && (!route || !brief.resolution) && <p className="vs-muted vs-plan-hint">可以先写方案，生成时再选择服务和清晰度。</p>}
                    </div>
                    {script && !editingScript && <div className="vs-script-preview"><ChatMarkdown content={script} /></div>}
                    {(editingScript || !script) && <textarea
                      aria-label="视频方案"
                      className="kv-textarea vs-script custom-scrollbar"
                      value={script}
                      onChange={(e) => {
                        beginEdit()
                        setScript(e.target.value)
                      }}
                      placeholder="先回到素材与要求设计方案，也可以在这里填写已有剧本。"
                    />}
                  </section>}
                  {isAnalysis && (
                    <section className="vs-panel">
                      <Button variant="primary" disabled={!script.trim()} onClick={useAnalysis}>{view === 'remake' ? '确认结构，进入制作' : '用我的商品仿拍'}</Button>
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
                          void guarded('保存参考模板…', async current => {
                            const t = await saved()
                            await api.videoStudioTemplate('template_save', {
                              id: t.id,
                              revision: t.revision,
                              name: templateName,
                            })
                            await refresh()
                            if (current()) selectView('templates')
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
                      <summary>查看生成提示词</summary>
                      <pre className="custom-scrollbar">
                        {dirty
                          ? script || brief.request || '请填写拍摄要求或方案。'
                          : task?.prompt || script || brief.request || '请填写拍摄要求或方案。'}
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
                      <Button onClick={() => selectView('settings')}>检查视频设置</Button>
                      <small>剧本、素材和提示词已保留，处理后可直接重试。</small>
                    </div>}
                    <div className="vs-actions">
                        <Button onClick={() => { workspaceVersion.current++; navigation.cancel(); setStep(0) }}>返回修改 / 更换服务</Button>
                        <Button
                          disabled={!native || !!busy || (!script.trim() && !brief.request.trim())}
                          onClick={() => void run('quote')}
                        >
                          刷新报价
                        </Button>
                        <Button
                          variant="primary"
                          disabled={
                            !native ||
                            !!busy ||
                            (!script.trim() && !brief.request.trim())
                          }
                          onClick={() => void run('submit')}
                        >
                          {task?.submission?.retryable || task?.status === 'uncertain' || task?.status === 'failed' ? '重试生成' : route === 'comfy' ? '开始生成' : '生成视频'}
                        </Button>
                      </div>
                    {dirty && (
                      <p className="vs-muted">
                        生成时会使用当前修改。
                      </p>
                    )}
                  </section>
                  {task?.remote && (
                    <section className="vs-panel">
                      <h3>{videoTaskStatus(task)}</h3>
                      {task.remote.id && <p className="vs-path">任务编号：{task.remote.id}</p>}
                      {task.status === 'uncertain' ? <>
                        <p role="alert">视频提交失败。可以重试，也可以返回修改素材、方案或更换服务。</p>
                        {task.submission?.httpStatus && <small>HTTP {task.submission.httpStatus}</small>}
                        <Button onClick={() => selectView('settings')}>检查视频设置</Button>
                      </> : task.error && <p role="alert">{task.error}</p>}
                      {task.remote.id && task.status !== 'succeeded' && (
                        <Button
                          disabled={!!busy}
                          onClick={() => void run('poll')}
                        >
                          <RefreshCw size={14} />
                          {task.status === 'running' && task.remote.download_url ? '恢复下载' : '查询进度 / 恢复结果'}
                        </Button>
                      )}
                      {task.remote.id && <p className="vs-muted">已保存任务编号，重新打开后可继续查询。</p>}
                    </section>
                  )}
                  {task?.output && (
                    <section className="vs-panel vs-result">
                      <div className="vs-result-preview">
                        {video ? (
                          <video
                            className="vs-video"
                            aria-label="成片预览"
                            src={video}
                            poster={poster || undefined}
                            controls
                            playsInline
                            preload="metadata"
                          />
                        ) : poster ? (
                          <img className="vs-video" src={poster} alt="成片首帧" />
                        ) : (
                          <span className="vs-muted">{previewError ? '预览不可用' : '正在加载预览…'}</span>
                        )}
                      </div>
                      <div className="vs-result-info">
                        <h3>生成结果</h3>
                        {task.media && <p className="vs-muted">{task.media.width}×{task.media.height} · {task.media.duration.toFixed(2)} 秒 · {task.media.hasAudio ? '有音轨' : '无音轨'}</p>}
                        {previewError && <p role="status" className="vs-muted">{previewError}</p>}
                        <div className="vs-actions">
                          <Button
                            onClick={() =>
                              void api.videoStudioOpen(task.id).catch(e => setError(String(e)))
                            }
                          >
                            <FolderOpen size={14} />
                            打开本地成片
                          </Button>
                          <Button
                            onClick={() =>
                              void api.videoStudioOpen(task.id, 'reveal').catch(e => setError(String(e)))
                            }
                          >
                            <ExternalLink size={14} />
                            在文件管理器中显示
                          </Button>
                        </div>
                      </div>
                    </section>
                  )}
                  {task?.output && (
                    <section className="vs-panel">
                      <h3>需要修改</h3>
                      <p className="vs-muted">
                        写下要改的地方，会按你的意见改写拍摄方案。确认后再生成，新成片覆盖这一条。
                      </p>
                      <Field label="需要修改的地方">
                        <textarea
                          className="kv-textarea custom-scrollbar"
                          rows={4}
                          value={reviseNote}
                          onChange={(e) => { workspaceVersion.current++; setReviseNote(e.target.value) }}
                          placeholder="例如：人物脸部特写太多，书包要完整入画；动作再自然一点。"
                        />
                      </Field>
                      <Button
                        variant="primary"
                        disabled={!native || !!busy || !reviseNote.trim()}
                        onClick={() => void revisePlan()}
                      >
                        <WandSparkles size={15} />
                        按意见改写拍摄方案
                      </Button>
                    </section>
                  )}
                  {task?.output && (
                    <section className="vs-panel">
                      <h3>保存为成片验证模板</h3>
                      <Field label="模板名称">
                        <input
                          className="kv-input"
                          placeholder="确认成片效果后，填写模板名称"
                          value={templateName}
                          onChange={(e) => setTemplateName(e.target.value)}
                        />
                      </Field>
                      <Button
                        disabled={!!busy || dirty || task?.status !== 'succeeded' || !templateName.trim()}
                        onClick={() =>
                          void guarded('保存成片模板…', async current => {
                            await api.videoStudioTemplate('template_save', {
                              id: task.id,
                              revision: task.revision,
                              name: templateName,
                              approvedOutput: true,
                            })
                            await refresh()
                            if (current()) selectView('templates')
                          })
                        }
                      >
                        确认成片并保存模板
                      </Button>
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
                            void guarded('恢复任务…', async current => {
                              const recovered = await api.videoStudioTask('recover', {
                                  id: task.id,
                                  revision: task.revision,
                                  remoteId: recoveryId,
                                })
                              if (current()) accept(recovered)
                              else setData(d => ({ ...d, tasks: [recovered, ...d.tasks.filter(t => t.id !== recovered.id)] }))
                            })
                          }
                        >
                          保存编号并恢复任务
                        </Button>
                      </details>
                    )}
                </>
              )}
            </>
          )}
        </main>
        {(view === 'creation' || view === 'analysis' || view === 'remake') && <ExecutionStatus
          active={!!busy || task?.status === 'running' || task?.status === 'submitting'}
          title={busy || (task ? videoTaskStatus(task) : 'AI 执行状态')}
          detail={error || (task?.status === 'uncertain' ? '提交失败，可重试、返回修改或更换服务。' : task?.error) || (busy
            ? '操作完成后会显示结果；等待时间不代表完成比例。'
            : task?.status === 'running' ? `${task.remote?.download_url ? '视频已生成，正在下载成片' : '视频服务正在生成'}${task.remote?.id ? ` · 任务编号 ${task.remote.id}` : ''}`
            : task?.output ? '成片已保存，可在生成与成片中查看。'
            : '当前步骤的执行状态将在这里显示。')}
        />}
        </div>
      </div>
    </div>
  )
}
