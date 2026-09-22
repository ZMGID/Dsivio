import type { ComponentType } from 'react'
import type { VideoFormProps } from './VideoFormFields'
import { VideoCreationForm } from './VideoCreationForm'
import { videoModel } from '../../../../data/videoModels'
import { useStudioNavigation } from '../../../studio/useStudioNavigation'
import { useChatRouteActive } from '../../../chatRouteVisibility'
import { useCallback, useEffect, useRef, useState, type DragEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import {
  ExternalLink,
  FolderOpen,
  Plus,
  RefreshCw,
  WandSparkles,
} from 'lucide-react'
import { api, isTauriRuntime } from '../../../../api/tauri'
import { Button, IconButton } from '../../../../components/Button'
import { Field } from '../../image/projects/StudioPanels'
import {
  videoTaskStatus,
  type VideoBootstrap,
  type VideoBrief,
  type VideoTask,
  type VideoTemplate,
} from './types'
import builtin from '../../../../../src-tauri/resources/plugins/dsvideo-plugin/skills/ecom-h3-video/templates/bedroom-ugc-product-presenter-15s.json'
import '../../image/projects/workbenchImage.css'
import '../../image/projects/studioLayout.css'
import './VideoStudio.css'
import { readVideoDrafts, readVideoTaskDraft, removeVideoTaskDraft, writeVideoDraft, newVideoDraftBrief, rememberVideoAssistant, rememberVideoSettings, preferredVideoResolution, videoResolutions, type VideoEntry } from './videoDrafts'
import { ChatMarkdown } from '../../../ChatMarkdown'
import { StudioToast } from '../../../studio/StudioToast'
import { useSharedDraft } from '../../../studio/useSharedDraft'
import {
  applyVideoStudioDrop,
  videoDropZoneFromPoint,
  type VideoDropZone,
} from './videoDrop'
import { useTaskLibrary } from '../../../studio/useTaskLibrary'
import { VideoTaskPanel } from './VideoTaskPanel'
import { ExecutionStatus } from '../../../studio/ExecutionStatus'
import { videoInputIssue } from './videoValidation'
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
  native: '视频模型',
}

export default function VideoProjectWorkspace({ feature = 'creation', BriefForm = VideoCreationForm }: { feature?: VideoEntry; BriefForm?: ComponentType<VideoFormProps> }) {
  const routeActive = useChatRouteActive()
  const native = isTauriRuntime()
  const library = useTaskLibrary('video', native)
  const [initial] = useState(() => readVideoDrafts()[feature])
  const [entry, setEntry] = useState<VideoEntry>(feature)
  const [draftSaved, setDraftSaved] = useState(true)
  const [editingScript, setEditingScript] = useState(false)
  const [data, setData] = useState<VideoBootstrap>(preview)
  const [runtimeCheck, setRuntimeCheck] = useState<
    'pending' | 'checking' | 'ready' | 'failed'
  >('pending')
  const [runtimeError, setRuntimeError] = useState('')
  const [view, setView] = useState<
    VideoEntry | 'tasks'
  >(feature)
  const [brief, setBrief] = useState<VideoBrief>(() => initial?.brief || newVideoDraftBrief(feature === 'creation' ? 'creation' : 'analysis'))
  const [task, setTask] = useState<VideoTask | undefined>(initial?.task)
  const [script, setScript] = useState(initial?.script || '')
  const [dirty, setDirty] = useState(initial?.dirty || false)
  const workspaceVersion = useRef(0)
  const [workspaceKey, setWorkspaceKey] = useState(0)
  const [foreground, setForeground] = useState({ key: -1, view: '', label: '', ticket: 0 })
  const foregroundTicket = useRef(0)
  const foregroundBusy = foreground.key === workspaceKey && foreground.view === view ? foreground.label : ''
  const navigation = useStudioNavigation()
  const selectView = (next: typeof view) => { workspaceVersion.current++; navigation.cancel(); setView(next) }
  const [operations, setOperations] = useState<Record<string, string>>({})
  const operationsRef = useRef(new Set<string>())
  const savingOperations = useRef(new Set<string>())
  const busy = foregroundBusy || ((view === 'creation' || view === 'analysis' || view === 'remake') && task ? operations[task.id] || '' : '')
  const [error, setError] = useState('')
  const [step, setStep] = useState(initial?.step || 0)
  const [templateName, setTemplateName] = useState('')
  const [reviseNote, setReviseNote] = useState('')
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
      const all = await api.workbenchVideoBootstrap()
      const next = { ...all, tasks: all.tasks.filter(t => feature === 'creation' ? t.brief.mode === 'creation' : t.brief.mode === 'analysis') }
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
  }, [native, feature])
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
      void api.workbenchVideoPoster(task.id)
        .then(url => { if (alive) setPoster(url) })
        .catch(() => {}) // A thumbnail failure must not block playback.
      void api
        .workbenchVideoPreview(task.id)
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
    if (view !== 'creation' && view !== 'analysis' && view !== 'remake') return
    setDraftSaved(writeVideoDraft(entry, { brief, task, script, step, dirty }))
  }, [entry, view, brief, task, script, step, dirty])

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
      const latest = operationsRef.current.has(t.id) ? t : await api.workbenchVideoTask('get', { id: t.id })
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
  async function saved(patch?: Partial<VideoBrief>, scriptOverride?: string, forGeneration = false) {
    const version = workspaceVersion.current
    const nextScript = scriptOverride ?? script
    const nextBrief = { ...brief, ...patch, name: brief.name || brief.request.trim().slice(0, 24) || (brief.mode === 'analysis' ? '视频拆解' : '视频创作') }
    let t = task && !['running', 'submitting'].includes(task.status) ? task : undefined
    if (t) {
      try {
        t = await api.workbenchVideoTask('get', { id: t.id })
      } catch (error) {
        if (!String(error).includes('VIDEO_TASK_NOT_FOUND') || ['submitting', 'running', 'uncertain'].includes(t.status)) throw error
        t = undefined
      }
    }
    if (t && ['running', 'submitting'].includes(t.status)) {
      if (forGeneration) throw new Error('任务仍在运行，请查询原任务结果')
      t = undefined
    }
    const creating = !t
    if (!t) t = await api.workbenchVideoTask('create', { brief: nextBrief })
    if (dirty || patch || scriptOverride !== undefined || (creating && nextScript.trim()))
      t = await api.workbenchVideoTask('save', {
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
    if (action === 'submit' && generationActive) return
    const saveKey = task?.id || `draft:${workspaceKey}`
    if (busy || savingOperations.current.has(saveKey) || (task && operationsRef.current.has(task.id))) return
    if (action === 'use_prompt' && !brief.request.trim()) { setError('请先填写已有的视频提示词'); return }
    const version = workspaceVersion.current
    if (action === 'plan' && !brief.request.trim() && !brief.template && !script.trim() && !brief.images.length) { setError('请填写拍摄要求或添加商品素材'); return }
    if (action === 'analyze' && !brief.source.trim()) { setError('请先添加参考视频'); return }
    if (['use_prompt', 'approve', 'prepare', 'quote', 'submit'].includes(action)) {
      const issue = videoInputIssue({ ...brief, ...patch }, action === 'analyze', view === 'remake', script) || (action === 'analyze' ? '' : (!brief.providerId || !brief.model ? '请选择工作台视频模型' : ''))
      if (issue) { setError(issue); return }
    }
    let t: VideoTask | undefined
    savingOperations.current.add(saveKey)
    await guarded('正在保存素材与要求…', async () => {
      t = (action === 'get' || action === 'poll') && task
        ? task : await saved(patch, action === 'use_prompt' ? brief.request : ['submit', 'quote'].includes(action) && !script.trim() ? brief.request : undefined, action === 'submit')
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
      if (action === 'get' || action === 'poll') t = await api.workbenchVideoTask('get', { id })
      if (action === 'poll' && (t.status === 'succeeded' || !t.remote?.id)) { acceptResult(t); return }
      if (['submit', 'quote'].includes(action) && (!t.approved || !t.prompt)) {
        t = await api.workbenchVideoTask('approve', { id, revision: t.revision })
        if (t.brief.route !== 'grok') {
          t = await api.workbenchVideoTask('prompt_result', { id, revision: t.revision, prompt: t.script })
        }
        acceptResult(t)
      }
      if (action !== 'save' && action !== 'get') {
        t = await api.workbenchVideoTask(action === 'use_prompt' ? 'approve' : action === 'submit' && t.status === 'failed' ? 'retry' : action, { id, revision: t.revision, ...(action === 'revise' ? { note: reviseNote.trim() } : { confirmSpend: action === 'submit' }) })
        acceptResult(t)
        if (action === 'approve' || action === 'use_prompt') {
          if (action === 'use_prompt' && t.brief.route !== 'grok') {
            t = await api.workbenchVideoTask('prompt_result', { id, revision: t.revision, prompt: t.script })
            acceptResult(t)
          } else if (t.brief.route !== 'grok') {
            setBusy('正在准备生成提示词…')
            t = await api.workbenchVideoTask('prepare', { id, revision: t.revision })
            acceptResult(t)
          }
          setStepIfCurrent(2)
          setBusy('正在查询报价…')
          try { t = await api.workbenchVideoTask('quote', { id, revision: t.revision }) } catch { /* Pricing is optional. */ }
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
  }, [native, feature])
  const keepOsDrop = (event: DragEvent, zone?: VideoDropZone) => {
    event.preventDefault()
    event.stopPropagation()
    if (zone) markDropTarget(zone)
  }
  const ActiveBriefForm = view === 'creation' ? VideoCreationForm : BriefForm
  const inputIssue = videoInputIssue(brief, isAnalysis, view === 'remake', script) || (isAnalysis ? '' : (!brief.providerId || !brief.model ? '请选择工作台视频模型' : ''))
  const route = brief.route
  const resolutions = videoModel(brief.model || '')?.resolutions || videoResolutions(brief)

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

        <div className="studio-workspace">
        <div className="workbench-page-head"><h2>{feature === 'creation' ? '短视频生成' : feature === 'remake' ? '爆款视频复刻' : '视频拆解'}</h2><div className="workbench-page-actions">
          <Button variant="ghost" onClick={() => selectView(entry)}>创作</Button>
          <Button variant="ghost" onClick={() => selectView('tasks')}>记录</Button>
          <a href="#chat/workbench/video-templates">视频模板</a>
        </div></div>
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
              currentId={task?.id} onOpen={openTask} onRefresh={() => refresh(true)} onNew={() => fresh(feature)} />
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
                <ActiveBriefForm brief={brief} data={data} busy={busy} native={native} dropActive={dropActive} dropTarget={dropTarget} change={change} keepOsDrop={keepOsDrop} guarded={guarded} pickImages={pickImages} setError={setError} inputIssue={inputIssue} resolutions={resolutions} run={run} />
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
                            await api.workbenchVideoTemplate('template_save', {
                              id: t.id,
                              revision: t.revision,
                              name: templateName,
                            })
                            await refresh()
                            if (current()) window.location.hash = '#chat/workbench/video-templates'
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
                      <a href="#chat/settings/media">检查媒体创作设置</a>
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
                            generationActive ||
                            task?.status === 'uncertain' ||
                            (!script.trim() && !brief.request.trim())
                          }
                          onClick={() => void run('submit')}
                        >
                          {task?.submission?.retryable || task?.status === 'failed' ? '重试生成' : route === 'comfy' ? '开始生成' : '生成视频'}
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
                        <p role="alert">提交结果尚未确认。请先核对供应商任务记录，填入回执后恢复查询，避免重复生成。</p>
                        {task.submission?.httpStatus && <small>HTTP {task.submission.httpStatus}</small>}
                        <a href="#chat/settings/media">检查媒体创作设置</a>
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
                              void api.workbenchVideoOpen(task.id).catch(e => setError(String(e)))
                            }
                          >
                            <FolderOpen size={14} />
                            打开本地成片
                          </Button>
                          <Button
                            onClick={() =>
                              void api.workbenchVideoOpen(task.id, 'reveal').catch(e => setError(String(e)))
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
                            await api.workbenchVideoTemplate('template_save', {
                              id: task.id,
                              revision: task.revision,
                              name: templateName,
                              approvedOutput: true,
                            })
                            await refresh()
                            if (current()) window.location.hash = '#chat/workbench/video-templates'
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
                              const recovered = await api.workbenchVideoTask('recover', {
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
          detail={error || (task?.status === 'uncertain' ? '提交结果尚未确认，请核对供应商任务记录并恢复查询。' : task?.error) || (busy
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
