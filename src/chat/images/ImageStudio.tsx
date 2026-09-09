import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open } from '@tauri-apps/plugin-dialog'
import {
  ArrowRight,
  Check,
  CheckCircle2,
  Download,
  FolderOpen,
  History,
  Image as ImageIcon,
  Layers3,
  Loader2,
  Palette,
  Plus,
  RefreshCw,
  Save,
  ScanLine,
  Settings2,
  Sparkles,
  Square,
  WandSparkles,
  X,
} from 'lucide-react'
import { api, isTauriRuntime } from '../../api/tauri'
import { Button, IconButton } from '../../components/Button'
import {
  AssetImage,
  ConfigPanel,
  Field,
  TemplatePanel,
} from './StudioPanels'
import {
  emptyBrief,
  FEATURES,
  latestResults,
  productGroup,
  sampleComplete,
  suggestImageTaskName,
  type ImageAction,
  type ImageBrief,
  type ImageConfig,
  type ImageFeature,
  type ImagePlan,
  type ImageProduct,
  type ImageResult,
  type ImageTask,
  type ImageTemplate,
} from './types'
import { builtinTemplates as initialTemplates } from './builtinTemplates'
import './imageStudio.css'
import './studioLayout.css'
import { DRAFT_KEY, readStudioDraft, storeStudioDraft } from './draft'
import { ImageBriefForm } from './ImageBriefForm'
import './imageFlow.css'
import { dropAsProducts, dropZoneFromPoint, type ImageDropZone } from './studioDrop'
import { ImageWorkflow } from './ImageWorkflow'
import { TaskPanel } from './TaskPanel'
import { useSharedDraft } from '../studio/useSharedDraft'
import { useTaskLibrary } from '../studio/useTaskLibrary'

const DEFAULT_CONFIG: ImageConfig = {
  providerId: '',
  model: '',
  protocol: 'openai',
  agentProviderId: '',
  agentModel: '',
}
type View = ImageFeature | 'templates' | 'tasks'
type Stage = 'brief' | 'plan' | 'results'

export default function ImageStudio() {
  const [view, setView] = useState<View>('gen')
  const [stage, setStage] = useState<Stage>('brief')
  const [brief, setBrief] = useState<ImageBrief>(
    () => readStudioDraft()?.brief || emptyBrief('gen'),
  )
  const [task, setTask] = useState<ImageTask | null>(null)
  const [tasks, setTasks] = useState<ImageTask[]>([])
  const [templates, setTemplates] = useState<ImageTemplate[]>(initialTemplates)
  const [config, setConfig] = useState<ImageConfig>(DEFAULT_CONFIG)
  const [group, setGroup] = useState('未分类')
  const [pending, setPending] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [selected, setSelected] = useState<ImageResult | null>(null)
  const [editNote, setEditNote] = useState('')
  const [editedPlans, setEditedPlans] = useState<ImagePlan[] | null>(null)
  const [exportOpen, setExportOpen] = useState(false)
  const [delivery, setDelivery] = useState({
    width: 800,
    height: 800,
    maxKb: 2048,
  })
  const [showHistory, setShowHistory] = useState(false)
  const [freezeName, setFreezeName] = useState('')
  const [dropActive, setDropActive] = useState(false)
  const [dropTarget, setDropTarget] = useState<ImageDropZone | null>(null)
  const [draftSaved, setDraftSaved] = useState(true)
  const running = task?.status === 'running'
  const busy = pending || running
  const dirty = task ? JSON.stringify(brief) !== JSON.stringify(task.brief) : true
  const native = isTauriRuntime()
  const library = useTaskLibrary('image', native)
  const report = useCallback(
    (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
    [],
  )
  const shared = useSharedDraft('image', 'main', native && !loading,
    { brief, taskId: task?.id, revision: task?.revision, plans: editedPlans },
    async (draft) => {
      // A chat-created task may not have reached the polled list yet.
      const saved = draft.taskId ? await api.imageStudioGet(draft.taskId) : null
      setBrief(draft.brief)
      setEditedPlans(draft.plans || null)
      setTask(saved || null)
      if (saved && draft.revision !== saved.revision) { setBrief(saved.brief); setEditedPlans(null) }
    }, busy)
  const syncCurrent = useRef({ task, dirty, editedPlans, busy })
  syncCurrent.current = { task, dirty, editedPlans, busy }
  const taskId = task?.id

  const adopt = useCallback((t: ImageTask) => {
    setTask(t)
    setBrief(t.brief)
    setEditedPlans(null)
    setTasks((all) =>
      [t, ...all.filter((x) => x.id !== t.id)].sort((a, b) =>
        b.updatedAt.localeCompare(a.updatedAt),
      ),
    )
  }, [])
  useEffect(() => {
    let alive = true
    if (!native) {
      setLoading(false)
      return
    }
    void api
      .imageStudioBootstrap()
      .then((data) => {
        if (alive) {
          setTasks(data.tasks)
          setTemplates(data.templates)
          setConfig(data.config)
          const draft = readStudioDraft()
          const saved = draft?.taskId ? data.tasks.find((t) => t.id === draft.taskId) : null
          if (saved) {
            setTask(saved)
            const restoreEdits = saved.revision === draft?.revision && saved.status !== 'running'
            setBrief(restoreEdits ? draft.brief : saved.brief)
            if (restoreEdits && draft.plans) setEditedPlans(draft.plans)
          }
        }
      })
      .catch(report)
      .finally(() => {
        if (alive) setLoading(false)
      })
    return () => {
      alive = false
    }
  }, [native, report])
  useEffect(() => {
    if (!loading)
      setDraftSaved(storeStudioDraft({
        brief,
        taskId: task?.id,
        revision: task?.revision,
        plans: editedPlans,
      }))
  }, [brief, task?.id, task?.revision, loading, editedPlans])
  useEffect(() => {
    if (!native || loading) return
    let alive = true
    let inFlight = false
    const refresh = async () => {
      if (inFlight || document.visibilityState === 'hidden') return
      inFlight = true
      try {
        const data = await api.imageStudioBootstrap()
        if (alive) {
          setTemplates(data.templates)
          setTasks(data.tasks)
          setConfig(data.config)
          const current = syncCurrent.current
          const updated = data.tasks.find(t => t.id === current.task?.id)
          if (updated && JSON.stringify(updated) !== JSON.stringify(current.task) && !current.busy) {
            if (!current.dirty && !current.editedPlans) adopt(updated)
            else setNotice('此任务已在聊天中更新。本地编辑已保留，请从任务列表重新打开最新版本后继续。')
          }
        }
      } catch {
        /* A template being saved in chat may be temporarily unavailable. */
      } finally {
        inFlight = false
      }
    }
    const timer = setInterval(() => void refresh(), 3000)
    window.addEventListener('focus', refresh)
    return () => {
      alive = false
      clearInterval(timer)
      window.removeEventListener('focus', refresh)
    }
  }, [native, loading, adopt])
  useEffect(() => {
    setView((current) =>
      current === 'templates' || current === 'tasks' ? current : brief.feature,
    )
  }, [brief.feature])
  useEffect(() => {
    if (!running || !taskId) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => {
      try {
        const current = await api.imageStudioGet(taskId)
        if (cancelled) return
        adopt(current)
        if (current.status === 'running') timer = setTimeout(() => void poll(), 1200)
        else if (current.progress.startsWith('规则模板已保存')) {
          const data = await api.imageStudioBootstrap()
          if (!cancelled) setTemplates(data.templates)
        }
      } catch (e) {
        if (!cancelled) {
          report(e)
          timer = setTimeout(() => void poll(), 3000)
        }
      }
    }
    timer = setTimeout(() => void poll(), 1000)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [taskId, running, adopt, report]) // A single non-overlapping poller survives status updates.

  const perform = async (fn: () => Promise<void>) => {
    if (!native) {
      setNotice('这是浏览器界面预览。请在 Dsivio 桌面窗口中导入素材、配置模型和生成图片。')
      return
    }
    setPending(true)
    setError('')
    setNotice('')
    try {
      await fn()
    } catch (e) {
      report(e)
    } finally {
      setPending(false)
    }
  }
  const save = async () => {
    if (task && !dirty) return task
    const saved = await api.imageStudioSave(
      {
        ...brief,
        name: brief.name.trim() || suggestImageTaskName(brief),
      },
      task?.id,
      task?.revision,
    )
    adopt(saved)
    try { localStorage.removeItem(DRAFT_KEY) } catch { /* Native task has already been saved. */ }
    return saved
  }
  const openTask = (item: ImageTask) =>
    void perform(async () => {
      if (
        dirty &&
        (brief.products.length || brief.requirement.trim() || brief.workflowInput?.sources.length)
      )
        await save()
      const latest = await api.imageStudioGet(item.id)
      adopt(latest)
      setView(latest.brief.feature)
      setStage(latest.results.length ? 'results' : latest.plans.length ? 'plan' : 'brief')
      setGroup(productGroup(latest.brief.products[0] || ({ category: '' } as ImageProduct)))
    })
  const switchView = async (next: View, template?: ImageTemplate) => {
    if (pending) return
    if (
      native &&
      dirty &&
      (brief.products.length || brief.requirement.trim() || brief.workflowInput?.sources.length)
    ) {
      try {
        await save()
      } catch (e) {
        report(e)
        return
      }
    }
    if (next === 'templates' || next === 'tasks') {
      setView(next)
      return
    }
    setView(next)
    setTask(null)
    setStage('brief')
    setGroup('未分类')
    setEditedPlans(null)
    setSelected(null)
    setNotice('')
    setError('')
    const b = emptyBrief(next)
    if (template) {
      b.templateId = template.id
      b.language = template.data.language || b.language
      b.ratio = template.data.output?.ratio || b.ratio
      b.resolution = template.data.output?.resolution || b.resolution
      b.count = template.data.slots.length
    }
    setBrief(b)
  }
  const act = (action: ImageAction) =>
    perform(async () => {
      const saved = action.kind === 'cancel' ? task : await save()
      if (!saved) return
      const current = await api.imageStudioAction(saved.id, saved.revision, {
        group,
        ...action,
      })
      adopt(current)
      if (action.kind === 'plan') setStage('plan')
      if (['start', 'sample', 'bulk', 'generate', 'retry', 'revise', 'resume'].includes(action.kind))
        setStage('results')
      if (action.kind === 'approve')
        setNotice('样品已确认。现在可以为这个分类的剩余商品规划并出图。')
    })
  const patch = (p: Partial<ImageBrief>) => setBrief((b) => ({ ...b, ...p }))
  const briefFeatureRef = useRef(brief.feature)
  briefFeatureRef.current = brief.feature
  const briefRef = useRef(brief)
  briefRef.current = brief
  const dropReadyRef = useRef({ accept: false, busy: false })
  dropReadyRef.current = {
    accept: view !== 'templates' && view !== 'tasks' && (view === 'workflow' || stage === 'brief'),
    busy,
  }
  const dropTargetRef = useRef<ImageDropZone | null>(null)
  const markDropTarget = (zone: ImageDropZone | null) => {
    dropTargetRef.current = zone
    setDropTarget(zone)
  }
  const dropTargetFromPosition = (position?: { x: number; y: number }) => {
    if (!position || !Number.isFinite(position.x) || !Number.isFinite(position.y)) return dropTargetRef.current
    const scale = window.devicePixelRatio || 1
    return dropZoneFromPoint(position.x / scale, position.y / scale) || dropTargetRef.current
  }
  const mergeImportedProducts = useCallback((asFolder: boolean, products: ImageProduct[]) => {
    setBrief((b) => {
      if (!asFolder && b.feature !== 'client' && b.products.length === 1) {
        const current = b.products[0]
        const added = products.flatMap((item) => item.assets)
        return {
          ...b,
          products: [
            {
              ...current,
              assets: [...current.assets, ...added],
              front: current.front || added[0]?.id || null,
            },
          ],
        }
      }
      return { ...b, products: [...b.products, ...products] }
    })
  }, [])
  const importFromPaths = (paths: string[], asFolder: boolean) =>
    perform(async () => {
      if (!paths.length) return
      const products = await api.imageStudioImport(
        paths,
        asFolder || briefFeatureRef.current === 'client',
      )
      mergeImportedProducts(asFolder, products)
    })
  const importWorkflowSources = (paths: string[]) =>
    perform(async () => {
      if (!paths.length) return
      const input = briefRef.current.workflowInput || { mode: 'smart' as const, sources: [] }
      const products = await api.imageStudioImport(paths, false)
      const assets = products.flatMap((item) => item.assets)
      if (input.sources.length + assets.length > 30) throw new Error('原始参考素材最多 30 张')
      setBrief((b) => {
        const current = b.workflowInput || { mode: 'smart' as const, sources: [] }
        return {
          ...b,
          workflowInput: { ...current, sources: [...current.sources, ...assets] },
        }
      })
    })
  const mergeReplaceSources = (products: ImageProduct[]) => {
    const sources = [...(briefRef.current.workflowInput?.sources || []), ...products.flatMap((item) => item.assets)]
    if (sources.length > 30) throw new Error('一套最多 30 张样图')
    patch({ templateId: null, workflowInput: { mode: 'replace', sources } })
  }
  const importReplaceSources = (paths: string[]) =>
    perform(async () => {
      if (!paths.length) return
      mergeReplaceSources(await api.imageStudioImport(paths, false))
    })
  const importWorkflowProducts = (paths: string[]) =>
    perform(async () => {
      if (!paths.length) return
      const products = await api.imageStudioImport(
        paths,
        dropAsProducts(paths, briefFeatureRef.current),
      )
      if (products.some((item) => item.assets.length > 16)) {
        throw new Error('每款商品最多 16 张参考图，请按商品分别选择')
      }
      if (briefRef.current.products.length + products.length > 200) {
        throw new Error('一个任务最多 200 款商品')
      }
      setBrief((b) => ({ ...b, products: [...b.products, ...products] }))
    })
  const importDropped = (paths: string[], zone: ImageDropZone | null = dropTargetRef.current) => {
    if (dropReadyRef.current.busy || !paths.length) return
    if (briefFeatureRef.current === 'workflow') {
      if (zone === 'products') {
        void importWorkflowProducts(paths)
        return
      }
      void importWorkflowSources(paths)
      return
    }
    if (briefFeatureRef.current === 'replace' && zone === 'examples') {
      void importReplaceSources(paths)
      return
    }
    void importFromPaths(paths, dropAsProducts(paths, briefFeatureRef.current))
  }
  const importDroppedRef = useRef(importDropped)
  importDroppedRef.current = importDropped
  useEffect(() => {
    if (!isTauriRuntime()) return
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
      .catch((err) => console.error('Image studio drag-drop listen failed:', err))
    return () => {
      cancelled = true
      setDropActive(false)
      unlisten?.()
    }
  }, [])
  const keepOsDrop = (event: DragEvent, zone?: ImageDropZone) => {
    event.preventDefault()
    event.stopPropagation()
    if (zone) markDropTarget(zone)
  }
  const importImages = (folder: boolean) =>
    perform(async () => {
      const paths = await open(
        folder
          ? {
              directory: true,
              multiple: true,
              title: '选择商品文件夹（每个子文件夹作为一个 SKU）',
            }
          : {
              multiple: true,
              title: '导入商品参考图',
              filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
            },
      )
      if (!paths) return
      const list = Array.isArray(paths) ? paths : [paths]
      const products = await api.imageStudioImport(
        list,
        folder || briefFeatureRef.current === 'client',
      )
      mergeImportedProducts(folder, products)
    })
  const updateTemplate = (t: ImageTemplate) =>
    setTemplates((all) => [...all.filter((x) => x.id !== t.id), t])
  const currentFeature = FEATURES.find((f) => f.id === brief.feature)!
  const groups = useMemo(() => [...new Set(brief.products.map(productGroup))], [brief.products])
  const activeGroup = groups.includes(group) ? group : groups[0] || '未分类'
  useEffect(() => {
    if (activeGroup !== group) setGroup(activeGroup)
  }, [activeGroup, group])
  const plans = editedPlans || task?.plans || []
  const groupPlans = plans.filter(
    (p) =>
      brief.feature === 'gen' ||
      productGroup(
        brief.products.find((x) => x.id === p.productId) || ({ category: '' } as ImageProduct),
      ) === group,
  )
  const results = (task ? (showHistory ? task.results : latestResults(task)) : []).filter((result) =>
    brief.feature === 'gen' || productGroup(brief.products.find((product) => product.id === result.productId) || ({ category: '' } as ImageProduct)) === group,
  )
  const complete = task ? sampleComplete(task, group) : false
  const approved = task?.approvedGroups.includes(group)
  const hasConfig = !!config.providerId && !!config.model
  const successCount = task ? latestResults(task).filter((r) => r.path).length : 0

  return (
    <section className="kv image-studio image-studio--polished" aria-label="图片工作台">
      {!native && (
        <div className="is-preview-note">
          界面预览模式 · 素材导入和生成需要在 Dsivio 桌面窗口中使用
        </div>
      )}
      {!draftSaved && <div role="alert" className="is-alert is-error">本机草稿未能保存，请先保存任务。
        <Button size="sm" disabled={busy} onClick={() => void perform(async () => { await save() })}>保存任务</Button>
      </div>}
      {error && (
        <div role="alert" className="is-alert is-error">
          <span>{error}</span>
          <IconButton label="关闭错误" onClick={() => setError('')}>
            <X size={16} />
          </IconButton>
        </div>
      )}
      {notice && (
        <div role="status" className="is-alert">
          <span>{notice}</span>
          <IconButton label="关闭提示" onClick={() => setNotice('')}>
            <X size={16} />
          </IconButton>
        </div>
      )}
      <div className="is-shell">
        <aside className="if-navigation custom-scrollbar">
          <span className="if-nav-label">开始创作</span>
          <nav className="if-feature-nav custom-scrollbar" aria-label="图片功能">
            {FEATURES.filter((feature) => feature.id !== 'workflow').map((feature) => (
              <button key={feature.id} type="button"
                className={view === feature.id ? 'active' : ''}
                title={feature.description}
                aria-current={view === feature.id ? 'page' : undefined}
                onClick={() => void switchView(feature.id)}>
                {feature.label}
              </button>
            ))}
          </nav>
          <nav className="if-tools-nav" aria-label="模板与记录">
            <span className="if-nav-label">模板与记录</span>
            <button type="button" className={view === 'workflow' ? 'active' : ''}
              aria-current={view === 'workflow' ? 'page' : undefined}
              onClick={() => void switchView('workflow')}><Sparkles size={15} /><span>制作模板</span></button>
            <button type="button" className={view === 'templates' ? 'active' : ''}
              aria-current={view === 'templates' ? 'page' : undefined}
              onClick={() => void switchView('templates')}>
              <FolderOpen size={15} /><span>模板库</span><span className="is-nav-count">{templates.length}</span>
            </button>
            <button type="button" className={view === 'tasks' ? 'active' : ''}
              aria-current={view === 'tasks' ? 'page' : undefined}
              onClick={() => void switchView('tasks')}>
              <History size={15} /><span>任务</span><span className="is-nav-count">{tasks.filter(t => t.status === 'running' || !library.organization[t.id]?.archived).length}</span>
            </button>
            <Button variant="ghost" aria-label="图片设置"
              title={hasConfig ? `图片设置 · ${config.model}` : '图片设置 · 待配置图片模型'}
              onClick={() => setSettingsOpen(true)}><Settings2 size={15} /><span>图片设置</span></Button>
          </nav>
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
          <div className="if-workspace">
          {view === 'templates' ? (
            <TemplatePanel
              templates={templates}
              onChange={updateTemplate}
              onUse={(t) => void switchView(t.data.mode === 'replace' ? 'replace' : 'smart', t)}
              report={report}
            />
          ) : view === 'tasks' ? (
            <TaskPanel tasks={tasks} loading={loading} currentId={task?.id} onOpen={openTask} library={library} disabled={pending}
              onNew={() => void switchView('gen')}
              onRefresh={async () => { const next = await api.imageStudioBootstrap(); setTasks(next.tasks); setTemplates(next.templates) }} />
          ) : view === 'workflow' ? (
            <ImageWorkflow
              key={task?.id || 'new-workflow'}
              brief={brief}
              task={task}
              busy={busy || loading}
              draftSaved={draftSaved}
              onChange={patch}
              onAction={act}
              perform={perform}
              onNew={() => void switchView('workflow')}
              onOpenResult={(result) => {
                setSelected(result)
                setEditNote('')
              }}
              onExport={() => setExportOpen(true)}
              dropActive={dropActive}
              dropTarget={dropTarget}
              onDropSurface={keepOsDrop}
            />
          ) : (
            <>
              <div className="is-work-heading">
                <div>
                  <h2>{currentFeature.label}</h2>
                  <p>{currentFeature.step}</p>
                </div>
                <Button size="sm" variant="ghost" disabled={pending}
                  onClick={() => void switchView(brief.feature)}><Plus size={15} />新任务</Button>
              </div>
              <div className="is-work-toolbar">
                <div className="is-stage-tabs" role="tablist" aria-label="制作阶段">
                  {(['brief', 'results'] as const).map((s, i) => (
                    <button
                      type="button"
                      role="tab"
                      aria-selected={stage === s}
                      key={s}
                      onClick={() => setStage(s)}
                    >
                      <span>{i + 1}</span>
                      {s === 'brief' ? '素材与要求' : '生成结果'}
                      {s === 'results' && successCount > 0 && <b>{successCount}</b>}
                    </button>
                  ))}
                </div>
                <div className="is-actions is-task-actions">
                  <span className="if-draft-status">{task && !dirty ? '已保存' : draftSaved ? '草稿保存在本机' : '草稿保存失败'}</span>
                </div>
              </div>
              {task && (
                <div className={`is-progress ${running ? 'running' : ''}`} role="status">
                  {running ? (
                    <Loader2 size={15} className="is-spinning" />
                  ) : task.error ? (
                    <Square size={13} />
                  ) : (
                    <CheckCircle2 size={15} />
                  )}
                  <span>{task.error || task.progress}</span>
                  {running && (
                    <Button
                      size="sm"
                      disabled={pending}
                      onClick={() => void act({ kind: 'cancel' })}
                    >
                      <Square size={12} />
                      停止
                    </Button>
                  )}
                </div>
              )}
              {groups.length > 1 && (
                <div className="is-group-tabs" aria-label="商品分类">
                  {groups.map((g) => (
                    <button
                      type="button"
                      key={g}
                      className={g === group ? 'active' : ''}
                      onClick={() => setGroup(g)}
                    >
                      {g}
                      <span>{brief.products.filter((p) => productGroup(p) === g).length}</span>
                      {task?.approvedGroups.includes(g) && <Check size={12} />}
                    </button>
                  ))}
                </div>
              )}
              {stage === 'brief' && (
                <ImageBriefForm brief={brief} templates={templates} busy={busy || loading}
                  dropActive={dropActive} onChange={patch}
                  onImport={(folder) => void importImages(folder)}
                  onImportExamples={() => void perform(async () => {
                    const paths = await open({ multiple: true, title: '按页面顺序选择现成套图', filters: [{ name: '图片', extensions: ['png', 'jpg', 'jpeg', 'webp'] }] })
                    if (!paths) return
                    mergeReplaceSources(await api.imageStudioImport(Array.isArray(paths) ? paths : [paths], false))
                  })}
                  dropTarget={dropTarget}
                  onDrop={keepOsDrop} onStart={() => void act({ kind: 'start' })} />
              )}
              {stage === 'plan' && (
                <section className="is-plan-page">
                  <div className="is-section-heading">
                    <div>
                      <h3>画面安排</h3>
                      <span className="is-count">{groupPlans.length} 页</span>
                    </div>
                    {groupPlans.length > 0 && (
                      <div className="is-actions">
                        <Button
                          size="sm"
                          disabled={busy || dirty || !!editedPlans}
                          onClick={() =>
                            void act({
                              kind: 'template',
                              productId: groupPlans[0]?.productId,
                            })
                          }
                        >
                          <Layers3 size={14} />
                          存为规则模板
                        </Button>
                        <Button
                          disabled={busy || !editedPlans}
                          onClick={() =>
                            void perform(async () => {
                              if (task && editedPlans)
                                adopt(
                                  await api.imageStudioSavePlans(
                                    task.id,
                                    task.revision,
                                    editedPlans,
                                  ),
                                )
                            })
                          }
                        >
                          <Save size={14} />
                          保存方案
                        </Button>
                        <Button
                          variant="primary"
                          disabled={busy || !!editedPlans || dirty}
                          onClick={() =>
                            void act({
                              kind: brief.feature === 'gen' ? 'generate' : 'sample',
                            })
                          }
                        >
                          <Sparkles size={15} />
                          {brief.feature === 'gen' ? '开始出图' : '生成两款样品'}
                        </Button>
                      </div>
                    )}
                  </div>
                  {editedPlans && (
                    <p className="is-muted">
                      保存方案会重新开始本次打样确认；已有图片仍保留在历史版本中。
                    </p>
                  )}
                  {!groupPlans.length ? (
                    <EmptyState
                      icon={<Palette size={34} />}
                      title={running ? 'Agent 正在整理画面方案' : '先把每一张图想清楚'}
                      text={
                        running
                          ? '商品、模板和画面要求正在逐项核对，方案会分款保存。'
                          : '完成素材与要求后，生成可编辑的页面方案。'
                      }
                      action={
                        <Button disabled={busy} onClick={() => setStage('brief')}>
                          返回素材与要求
                        </Button>
                      }
                    />
                  ) : (
                    groupPlans.map((plan) => {
                      const p = brief.products.find((p) => p.id === plan.productId)
                      return (
                        <article className="is-plan-card" key={`${plan.productId}/${plan.slotId}`}>
                          <div className="is-plan-visual">
                            {p?.assets[0] ? (
                              <AssetImage
                                path={
                                  p.assets.find((a) => a.id === p.front)?.path || p.assets[0].path
                                }
                                name={p.name}
                              />
                            ) : (
                              <ImageIcon size={30} />
                            )}
                            <span>{plan.slotId}</span>
                          </div>
                          <div className="is-plan-body">
                            <div className="is-section-heading">
                              <div>
                                <h3>{plan.purpose}</h3>
                                <span className="is-muted">{p?.name}</span>
                              </div>
                              <span className="is-count">{plan.refs.length} 张参考图</span>
                            </div>
                            {plan.copy && (
                              <div className="is-copy">
                                <span>图内文案</span>
                                <p>{plan.copy}</p>
                              </div>
                            )}
                            <details className="if-more"><summary>编辑详细提示词</summary><Field label="完整画面提示词（包括需要修改的图内文案）">
                              <textarea
                                className="kv-textarea custom-scrollbar"
                                rows={5}
                                disabled={busy}
                                value={plan.prompt}
                                onChange={(e) =>
                                  setEditedPlans(
                                    plans.map((x) =>
                                      x.productId === plan.productId && x.slotId === plan.slotId
                                        ? { ...x, prompt: e.target.value }
                                        : x,
                                    ),
                                  )
                                }
                              />
                            </Field></details>
                          </div>
                        </article>
                      )
                    })
                  )}
                </section>
              )}
              {stage === 'results' && (
                <section className="is-results-page">
                  <div className="is-section-heading">
                    <div>
                      <h3>生成结果</h3>
                      <span className="is-count">{results.filter((result) => result.path).length} 张</span>
                    </div>
                    <div className="is-actions">
                      {!busy && task && (task.error || task.status === 'stopped') && results.length > 0 && <Button size="sm" onClick={() => void act({ kind: 'start' })}><RefreshCw size={14} />继续生成</Button>}
                      {!!plans.length && <Button size="sm" variant="ghost" onClick={() => setStage('plan')}>查看画面安排</Button>}
                      <Button
                        size="sm"
                        variant={showHistory ? 'default' : 'ghost'}
                        onClick={() => setShowHistory((v) => !v)}
                      >
                        <History size={14} />
                        历史版本
                      </Button>
                      <Button
                        size="sm"
                        disabled={!successCount}
                        onClick={() => setExportOpen(true)}
                      >
                        <Download size={14} />
                        导出交付图
                      </Button>
                    </div>
                  </div>
                  {brief.feature !== 'gen' && task && brief.products.filter((p) => productGroup(p) === group).length > 2 && (
                    <div className={`is-sample-gate ${approved ? 'approved' : ''}`}>
                      <div>
                        <CheckCircle2 size={20} />
                        <span>
                          <strong>
                            {approved ? `${group} · 样品已通过` : `${group} · 先检查样品`}
                          </strong>
                          <small>
                            核对商品外观、文字拼写、版式与正反面，再确认这个分类继续批量。
                          </small>
                        </span>
                      </div>
                      <Button
                        variant="primary"
                        disabled={busy || dirty || !complete}
                        onClick={() => void act({ kind: approved ? 'bulk' : 'approve' })}
                      >
                        {approved ? '生成剩余商品' : '确认样品效果'}
                        <ArrowRight size={14} />
                      </Button>
                    </div>
                  )}
                  {!results.length ? (
                    <EmptyState
                      icon={<ImageIcon size={36} />}
                      title={running ? '正在把方案变成图片' : '你的成图会出现在这里'}
                      text={running ? '正在处理素材并生成图片，完成后会自动保存。' : '添加素材和要求，就可以开始生成。'}
                      action={
                        <Button disabled={busy} onClick={() => task?.error ? void act({ kind: 'start' }) : setStage('brief')}>
                          {task?.error ? '重新尝试' : '返回素材与要求'}
                        </Button>
                      }
                    />
                  ) : (
                    <div className="is-result-grid">
                      {results.map((r) => {
                        const p = brief.products.find((p) => p.id === r.productId)
                        const plan = plans.find(
                          (x) => x.productId === r.productId && x.slotId === r.slotId,
                        )
                        return (
                          <article
                            className={`is-result-card ${r.error ? 'failed' : ''}`}
                            key={r.id}
                          >
                            <button
                              type="button"
                              className="is-result-preview"
                              disabled={!r.path}
                              onClick={() => {
                                setSelected(r)
                                setEditNote('')
                                setFreezeName(`${p?.name || '商品'} · 样图模板`)
                              }}
                            >
                              {r.path ? (
                                <AssetImage path={r.path} name={plan?.purpose || r.slotId} />
                              ) : (
                                <div>
                                  {running && !r.error ? (
                                    <Loader2 size={26} className="is-spinning" />
                                  ) : (
                                    <ImageIcon size={28} />
                                  )}
                                  <p>
                                    {r.error
                                      ? '本张图片未完成'
                                      : running
                                        ? '正在生成'
                                        : r.remoteId
                                          ? '远程任务已保存'
                                          : '等待图片结果'}
                                  </p>
                                  <small>{r.error}</small>
                                </div>
                              )}
                              <span className="is-result-slot">{r.slotId}</span>
                              {r.revision !== task?.revision && (
                                <span className="is-old-version">历史需求版本</span>
                              )}
                            </button>
                            <div className="is-result-meta">
                              <h3>{plan?.purpose || r.slotId}</h3>
                              <p>
                                {p?.name}{' '}
                                {r.path && (
                                  <span>
                                    {r.width} × {r.height}
                                  </span>
                                )}
                              </p>
                              <div className="is-actions">
                                {r.path ? (
                                  <Button
                                    size="sm"
                                    onClick={() => {
                                      setSelected(r)
                                      setEditNote('')
                                      setFreezeName(`${p?.name || '商品'} · 样图模板`)
                                    }}
                                  >
                                    查看 / 修改
                                  </Button>
                                ) : (
                                  <Button
                                    size="sm"
                                    disabled={busy || dirty || r.revision !== task?.revision}
                                    onClick={() =>
                                      void act(
                                        r.remoteId && !r.error?.startsWith('远程图片任务失败')
                                          ? { kind: 'resume', resultId: r.id }
                                          : {
                                              kind: 'retry',
                                              productId: r.productId,
                                              slotId: r.slotId,
                                            },
                                      )
                                    }
                                  >
                                    <RefreshCw size={13} />
                                    {r.remoteId && !r.error?.startsWith('远程图片任务失败')
                                      ? '恢复查询'
                                      : '单页重试'}
                                  </Button>
                                )}
                                {r.review && (
                                  <span className="is-reviewed">
                                    <Check size={12} />
                                    已质检
                                  </span>
                                )}
                              </div>
                            </div>
                          </article>
                        )
                      })}
                    </div>
                  )}
                </section>
              )}
            </>
          )}
          </div>
        </main>
      </div>
      {settingsOpen && (
        <ConfigPanel
          config={config}
          onClose={() => setSettingsOpen(false)}
          onSave={async (c) => {
            if (!native) throw new Error('请在 Dsivio 桌面窗口中保存配置')
            await api.imageStudioConfig(c)
            setConfig(c)
            setSettingsOpen(false)
          }}
        />
      )}
      {selected && (
        <div className="kv-modal-backdrop kv-modal-backdrop--portal is-overlay">
          <section
            className="kv-modal is-result-dialog custom-scrollbar"
            role="dialog"
            aria-modal="true"
            aria-label="查看和修改图片"
          >
            <div className="is-lightbox-image">
              {selected.path && <AssetImage path={selected.path} name={selected.slotId} large />}
            </div>
            <div className="is-lightbox-tools custom-scrollbar">
              <div className="is-section-heading">
                <div>
                  <h2>{selected.slotId} · 图片版本</h2>
                </div>
                <IconButton label="关闭图片" onClick={() => setSelected(null)}>
                  <X size={18} />
                </IconButton>
              </div>
              <p className="is-muted">
                {selected.width} × {selected.height} · 原图保留，每次修改生成新版本
              </p>
              <div className="is-actions">
                <Button
                  size="sm"
                  onClick={() =>
                    void perform(async () => {
                      if (selected.path) await api.imageStudioOpen(selected.path)
                    })
                  }
                >
                  <FolderOpen size={14} />
                  打开原图
                </Button>
                <Button
                  size="sm"
                  disabled={busy || dirty || selected.revision !== task?.revision}
                  onClick={() => {
                    void act({ kind: 'review', resultId: selected.id })
                    setSelected(null)
                  }}
                >
                  <ScanLine size={14} />
                  Agent 质检
                </Button>
              </div>
              {(task?.results.find((r) => r.id === selected.id)?.review || selected.review) && (
                <div className="is-review">
                  <strong>质检意见</strong>
                  <p>
                    {task?.results.find((r) => r.id === selected.id)?.review || selected.review}
                  </p>
                </div>
              )}
              <Field label="只修改这一张">
                <textarea
                  className="kv-textarea custom-scrollbar"
                  rows={4}
                  value={editNote}
                  onChange={(e) => setEditNote(e.target.value)}
                  placeholder="例如：产品不变，背景换成暖灰色，标题缩小一点"
                />
              </Field>
              <Button
                variant="primary"
                disabled={busy || dirty || !editNote.trim() || selected.revision !== task?.revision}
                onClick={() => {
                  void act({
                    kind: 'revise',
                    resultId: selected.id,
                    productId: selected.productId,
                    slotId: selected.slotId,
                    note: editNote,
                  })
                  setSelected(null)
                }}
              >
                <WandSparkles size={15} />
                生成修改版本
              </Button>
              <details className="is-source-prompt">
                <summary>查看生成提示词</summary>
                <p>{selected.prompt}</p>
              </details>
              {brief.feature === 'workflow' && (
                <p className="is-muted">
                  要让后续商品也采用同样的修改，请回到流程中填写「对共用规则的修改意见」。
                </p>
              )}
              {brief.feature !== 'workflow' && (
                <details className="if-more">
                  <summary>保存为模板，下次继续用</summary>
                  <Field label="把这一款完整套图存成模板">
                    <input
                      className="kv-input"
                      value={freezeName}
                      onChange={(e) => setFreezeName(e.target.value)}
                      placeholder="模板名称"
                    />
                  </Field>
                  <Button
                    disabled={
                      busy || dirty || !freezeName.trim() || selected.revision !== task?.revision
                    }
                    onClick={() =>
                      void perform(async () => {
                        if (task) {
                          updateTemplate(
                            await api.imageStudioFreeze(task.id, selected.productId, freezeName),
                          )
                          setSelected(null)
                          setNotice('已保存为样图模板，请在模板库检查文案和商品专属规则后复用。')
                        }
                      })
                    }
                  >
                    <Layers3 size={15} />
                    保存样图模板
                  </Button>
                </details>
              )}
            </div>
          </section>
        </div>
      )}
      {exportOpen && (
        <div className="kv-modal-backdrop kv-modal-backdrop--portal is-overlay">
          <section
            className="kv-modal is-dialog custom-scrollbar"
            role="dialog"
            aria-modal="true"
            aria-label="导出交付图"
          >
            <div className="is-section-heading">
              <div>
                <h2>导出交付图</h2>
              </div>
              <IconButton label="关闭导出" onClick={() => setExportOpen(false)}>
                <X size={18} />
              </IconButton>
            </div>
            <p className="is-muted">
              导出当前需求版本每页的最新成图。原图不会被覆盖，文件夹包含商品与图片清单。
            </p>
            <Field label="交付保存位置">
              <input className="kv-input" readOnly value={task?.outputDirectory ? `${task.outputDirectory}/deliveries` : '首次导出时，在图片保存位置为此任务创建文件夹'} />
            </Field>
            <div className="is-two-cols">
              <Field label="交付宽度 px">
                <input
                  className="kv-input"
                  type="number"
                  min={0}
                  max={8192}
                  value={delivery.width}
                  onChange={(e) => setDelivery((d) => ({ ...d, width: +e.target.value }))}
                />
              </Field>
              <Field label="交付高度 px">
                <input
                  className="kv-input"
                  type="number"
                  min={0}
                  max={8192}
                  value={delivery.height}
                  onChange={(e) => setDelivery((d) => ({ ...d, height: +e.target.value }))}
                />
              </Field>
            </div>
            <Field label="每张最大 KB（0 为不限制）">
              <input
                className="kv-input"
                type="number"
                min={0}
                value={delivery.maxKb}
                onChange={(e) => setDelivery((d) => ({ ...d, maxKb: +e.target.value }))}
              />
            </Field>
            <p className="is-muted is-small">
              调整尺寸会等比缩放并补白，导出 JPG。宽、高和大小全部设为 0，可原样复制生成文件。
            </p>
            <div className="is-dialog-actions">
              <Button onClick={() => setDelivery({ width: 0, height: 0, maxKb: 0 })}>
                使用原图规格
              </Button>
              <Button
                variant="primary"
                disabled={pending}
                onClick={() =>
                  void perform(async () => {
                    if (!task) return
                    const path = await api.imageStudioExport(
                      task.id,
                      '',
                      delivery.width,
                      delivery.height,
                      delivery.maxKb,
                    )
                    setExportOpen(false)
                    setNotice(`已导出到 ${path}`)
                  })
                }
              >
                <Download size={15} />
                导出到任务文件夹
              </Button>
            </div>
          </section>
        </div>
      )}
    </section>
  )
}

function EmptyState({
  icon,
  title,
  text,
  action,
}: {
  icon: React.ReactNode
  title: string
  text: string
  action: React.ReactNode
}) {
  return (
    <div className="is-empty-state">
      <span>{icon}</span>
      <h3>{title}</h3>
      <p>{text}</p>
      {action}
    </div>
  )
}
