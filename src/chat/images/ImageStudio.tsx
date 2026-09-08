import { useCallback, useEffect, useMemo, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import {
  ArrowRight,
  Check,
  CheckCircle2,
  ChevronRight,
  Download,
  FileImage,
  FolderOpen,
  Grid2X2,
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
  ImageLanguageSelect,
  StudioSelect,
  TemplatePanel,
} from './StudioPanels'
import {
  emptyBrief,
  FEATURES,
  latestResults,
  productGroup,
  sampleComplete,
  SHOTS,
  type ImageAction,
  type ImageBrief,
  type ImageConfig,
  type ImageFeature,
  type ImagePlan,
  type ImageProduct,
  type ImageProvider,
  type ImageResult,
  type ImageTask,
  type ImageTemplate,
} from './types'
import { builtinTemplates as initialTemplates } from './builtinTemplates'
import './imageStudio.css'
import './studioLayout.css'
import { DRAFT_KEY, readStudioDraft, storeStudioDraft } from './draft'

const ICONS = [WandSparkles, ScanLine, Layers3, Palette, Grid2X2]
const DEFAULT_CONFIG: ImageConfig = {
  providerId: '',
  model: '',
  protocol: 'openai',
  agentProviderId: '',
  agentModel: '',
}
type View = ImageFeature | 'templates'
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
  const [providers, setProviders] = useState<ImageProvider[]>([])
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
  const running = task?.status === 'running'
  const busy = pending || running
  const dirty = task ? JSON.stringify(brief) !== JSON.stringify(task.brief) : true
  const native = isTauriRuntime()
  const report = useCallback(
    (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
    [],
  )
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
          setProviders(data.providers)
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
      storeStudioDraft({
        brief,
        taskId: task?.id,
        revision: task?.revision,
        plans: editedPlans,
      })
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
          setProviders(data.providers)
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
  }, [native, loading])
  useEffect(() => {
    setView(brief.feature)
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
        name:
          brief.name.trim() ||
          `${FEATURES.find((f) => f.id === brief.feature)?.label} · ${new Date().toLocaleDateString()}`,
      },
      task?.id,
      task?.revision,
    )
    adopt(saved)
    localStorage.removeItem(DRAFT_KEY)
    return saved
  }
  const switchView = async (next: View, template?: ImageTemplate) => {
    if (pending) return
    if (native && dirty && (brief.products.length || brief.requirement.trim())) {
      try {
        await save()
      } catch (e) {
        report(e)
        return
      }
    }
    if (next === 'templates') {
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
      if (['sample', 'bulk', 'generate', 'retry', 'revise', 'resume'].includes(action.kind))
        setStage('results')
      if (action.kind === 'approve')
        setNotice('样品已确认。现在可以为这个分类的剩余商品规划并出图。')
    })
  const patch = (p: Partial<ImageBrief>) => setBrief((b) => ({ ...b, ...p }))
  const patchProduct = (id: string, p: Partial<ImageProduct>) =>
    setBrief((b) => ({
      ...b,
      products: b.products.map((x) => (x.id === id ? { ...x, ...p } : x)),
    }))
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
      const products = await api.imageStudioImport(
        Array.isArray(paths) ? paths : [paths],
        folder || brief.feature === 'client',
      )
      setBrief((b) => {
        if (!folder && b.feature !== 'client' && b.products.length === 1) {
          const p = b.products[0],
            added = products.flatMap((p) => p.assets)
          return {
            ...b,
            products: [
              {
                ...p,
                assets: [...p.assets, ...added],
                front: p.front || added[0]?.id || null,
              },
            ],
          }
        }
        return { ...b, products: [...b.products, ...products] }
      })
    })
  const updateTemplate = (t: ImageTemplate) =>
    setTemplates((all) => [...all.filter((x) => x.id !== t.id), t])
  const currentFeature = FEATURES.find((f) => f.id === brief.feature)!
  const groups = useMemo(() => [...new Set(brief.products.map(productGroup))], [brief.products])
  const activeGroup = groups.includes(group) ? group : groups[0] || '未分类'
  useEffect(() => {
    if (activeGroup !== group) setGroup(activeGroup)
  }, [activeGroup, group])
  const visibleProducts =
    brief.feature !== 'gen'
      ? brief.products.filter((p) => productGroup(p) === group)
      : brief.products
  const template = templates.find((t) => t.id === brief.templateId)
  const plans = editedPlans || task?.plans || []
  const groupPlans = plans.filter(
    (p) =>
      brief.feature === 'gen' ||
      productGroup(
        brief.products.find((x) => x.id === p.productId) || ({ category: '' } as ImageProduct),
      ) === group,
  )
  const results = task ? (showHistory ? task.results : latestResults(task)) : []
  const complete = task ? sampleComplete(task, group) : false
  const approved = task?.approvedGroups.includes(group)
  const hasConfig = !!config.providerId && !!config.model
  const successCount = task ? latestResults(task).filter((r) => r.path).length : 0

  return (
    <section className="kv image-studio" aria-label="图片工作台">
      {!native && (
        <div className="is-preview-note">
          界面预览模式 · 素材导入和生成需要在 Dsivio 桌面窗口中使用
        </div>
      )}
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
        <aside className="is-rail custom-scrollbar">
          <span className="is-rail-label">开始创作</span>
          <nav aria-label="图片功能">
            {FEATURES.map((f, i) => {
              const Icon = ICONS[i]
              return (
                <button
                  key={f.id}
                  type="button"
                  className={view === f.id ? 'active' : ''}
                  onClick={() => void switchView(f.id)}
                >
                  <Icon size={18} />
                  <span>{f.label}</span>
                  {view === f.id && <ChevronRight size={13} />}
                </button>
              )
            })}
            <button
              type="button"
              className={view === 'templates' ? 'active' : ''}
              onClick={() => void switchView('templates')}
            >
              <FolderOpen size={18} />
              <span>模板库</span>
              <span className="is-nav-count">{templates.length}</span>
            </button>
          </nav>
          <div className="is-rail-history custom-scrollbar">
            <div className="is-rail-label">
              <span>最近任务</span>
              <History size={13} />
            </div>
            {loading && <span className="is-muted">正在加载…</span>}
            {!loading && tasks.length === 0 && <p>保存的图片任务会留在这里，下次接着做。</p>}
            {tasks.slice(0, 20).map((t) => (
              <button
                type="button"
                key={t.id}
                className={task?.id === t.id ? 'active' : ''}
                onClick={() =>
                  void perform(async () => {
                    if (dirty && (brief.products.length || brief.requirement.trim())) await save()
                    const latest = await api.imageStudioGet(t.id)
                    adopt(latest)
                    setView(latest.brief.feature)
                    setStage(
                      latest.results.length ? 'results' : latest.plans.length ? 'plan' : 'brief',
                    )
                    setGroup(
                      productGroup(latest.brief.products[0] || ({ category: '' } as ImageProduct)),
                    )
                  })
                }
              >
                <span className={`is-history-dot ${t.status}`} />
                <span>{t.brief.name || '未命名任务'}</span>
              </button>
            ))}
          </div>
          <div className="is-rail-foot">
            <Button
              variant="ghost"
              size="sm"
              aria-label="图片设置"
              title={hasConfig ? `图片设置 · ${config.model}` : '图片设置 · 待配置图片模型'}
              onClick={() => setSettingsOpen(true)}
            >
              <Settings2 size={16} />
              <span>图片设置</span>
            </Button>
          </div>
        </aside>
        <main className="is-main custom-scrollbar">
          {view === 'templates' ? (
            <TemplatePanel
              templates={templates}
              onChange={updateTemplate}
              onUse={(t) => void switchView(t.data.mode === 'replace' ? 'replace' : 'smart', t)}
              report={report}
            />
          ) : (
            <>
              <div className="is-work-heading">
                <div>
                  <span className="is-kicker">{currentFeature.description}</span>
                  <h2>{currentFeature.label}</h2>
                  <p>{currentFeature.step}</p>
                </div>
              </div>
              <div className="is-work-toolbar">
                <div className="is-stage-tabs" role="tablist" aria-label="制作阶段">
                  {(['brief', 'plan', 'results'] as const).map((s, i) => (
                    <button
                      type="button"
                      role="tab"
                      aria-selected={stage === s}
                      key={s}
                      onClick={() => setStage(s)}
                    >
                      <span>{i + 1}</span>
                      {['素材与要求', '画面方案', '生成结果'][i]}
                      {s === 'results' && successCount > 0 && <b>{successCount}</b>}
                    </button>
                  ))}
                </div>
                <div className="is-actions is-task-actions">
                  <Button
                    size="sm"
                    disabled={pending}
                    onClick={() => void switchView(brief.feature)}
                  >
                    <Plus size={14} />
                    新任务
                  </Button>
                  <Button
                    size="sm"
                    disabled={busy}
                    onClick={() =>
                      void perform(async () => {
                        await save()
                        setNotice('任务已保存')
                      })
                    }
                  >
                    <Save size={14} />
                    {task && !dirty ? '已保存' : '保存任务'}
                  </Button>
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
                <div className="is-brief-layout">
                  <div className="is-editor-column">
                    <section className="is-section">
                      <div className="is-section-heading">
                        <div>
                          <span className="is-step-number">01</span>
                          <h3>商品素材</h3>
                          <span className="is-count">{brief.products.length} 款</span>
                        </div>
                        <div className="is-actions">
                          <Button size="sm" disabled={busy} onClick={() => void importImages(true)}>
                            <FolderOpen size={14} />
                            商品文件夹
                          </Button>
                          <Button
                            size="sm"
                            disabled={busy}
                            onClick={() => void importImages(false)}
                          >
                            <Plus size={14} />
                            添加图片
                          </Button>
                        </div>
                      </div>
                      {!brief.products.length ? (
                        <div className="is-upload-area">
                          <div className="is-upload-illustration">
                            <FileImage size={35} strokeWidth={1.1} />
                            <span>
                              <Plus size={14} />
                            </span>
                          </div>
                          <h3>放进你的商品，开始创作</h3>
                          <p>支持 PNG、JPG、WebP · 一款商品可以有多张参考图</p>
                          <Button disabled={busy} onClick={() => void importImages(false)}>
                            选择商品图片
                            <ArrowRight size={14} />
                          </Button>
                          <small>
                            {brief.feature === 'gen'
                              ? '也可以不传图片，直接描述你想要的画面'
                              : '批量导入时，每个商品子文件夹作为一个 SKU'}
                          </small>
                        </div>
                      ) : (
                        <div className="is-products">
                          {visibleProducts.map((p, index) => {
                            const pt = templates.find(
                              (t) => t.id === (p.templateId || brief.templateId),
                            )
                            return (
                              <article className="is-product" key={p.id}>
                                <div className="is-product-heading">
                                  <span className="is-step-number">{index + 1}</span>
                                  <input
                                    className="kv-input"
                                    aria-label="商品名称 / SKU"
                                    disabled={busy}
                                    value={p.name}
                                    onChange={(e) =>
                                      patchProduct(p.id, {
                                        name: e.target.value,
                                      })
                                    }
                                  />
                                  {index < 2 && brief.feature !== 'gen' && (
                                    <span className="is-sample-badge">样品</span>
                                  )}
                                  <IconButton
                                    label={`移除 ${p.name}`}
                                    disabled={busy}
                                    onClick={() =>
                                      patch({
                                        products: brief.products.filter((x) => x.id !== p.id),
                                      })
                                    }
                                  >
                                    <X size={14} />
                                  </IconButton>
                                </div>
                                <div className="is-asset-strip custom-scrollbar">
                                  {p.assets.map((a) => (
                                    <div className="is-asset" key={a.id}>
                                      <AssetImage path={a.path} name={a.name} />
                                      <span>
                                        {p.front === a.id
                                          ? '正面'
                                          : p.back === a.id
                                            ? '背面'
                                            : '参考'}
                                      </span>
                                      <small title={a.name}>{a.name}</small>
                                      <IconButton
                                        label={`移除素材 ${a.name}`}
                                        disabled={busy}
                                        onClick={() =>
                                          patchProduct(p.id, {
                                            assets: p.assets.filter((x) => x.id !== a.id),
                                            front: p.front === a.id ? null : p.front,
                                            back: p.back === a.id ? null : p.back,
                                          })
                                        }
                                      >
                                        <X size={11} />
                                      </IconButton>
                                    </div>
                                  ))}
                                </div>
                                <div className="is-two-cols">
                                  <Field label="商品正面">
                                    <StudioSelect
                                      disabled={busy}
                                      value={p.front || ''}
                                      onChange={(e) =>
                                        patchProduct(p.id, {
                                          front: e.target.value || null,
                                        })
                                      }
                                    >
                                      <option value="">选择正面图</option>
                                      {p.assets.map((a) => (
                                        <option key={a.id} value={a.id}>
                                          {a.name}
                                        </option>
                                      ))}
                                    </StudioSelect>
                                  </Field>
                                  <Field label="真实背面（需要时填写）">
                                    <StudioSelect
                                      disabled={busy}
                                      value={p.back || ''}
                                      onChange={(e) =>
                                        patchProduct(p.id, {
                                          back: e.target.value || null,
                                        })
                                      }
                                    >
                                      <option value="">未提供背面图</option>
                                      {p.assets.map((a) => (
                                        <option key={a.id} value={a.id}>
                                          {a.name}
                                        </option>
                                      ))}
                                    </StudioSelect>
                                  </Field>
                                </div>
                                {brief.feature === 'client' && (
                                  <div className="is-two-cols">
                                    <Field label="所属分类">
                                      <input
                                        className="kv-input"
                                        disabled={busy}
                                        value={p.category}
                                        onChange={(e) =>
                                          patchProduct(p.id, {
                                            category: e.target.value,
                                          })
                                        }
                                      />
                                    </Field>
                                    <Field label="这一款的模板">
                                      <StudioSelect
                                        disabled={busy}
                                        value={p.templateId || ''}
                                        onChange={(e) =>
                                          patchProduct(p.id, {
                                            templateId: e.target.value || null,
                                          })
                                        }
                                      >
                                        <option value="">使用通用模板</option>
                                        {templates.map((t) => (
                                          <option value={t.id} key={t.id}>
                                            {t.data.name}
                                          </option>
                                        ))}
                                      </StudioSelect>
                                    </Field>
                                  </div>
                                )}
                                {!!pt?.data.product_kinds && (
                                  <Field label="款型分支">
                                    <StudioSelect
                                      disabled={busy}
                                      value={p.kind}
                                      onChange={(e) =>
                                        patchProduct(p.id, {
                                          kind: e.target.value,
                                        })
                                      }
                                    >
                                      <option value="">选择商品款型</option>
                                      {Object.keys(pt.data.product_kinds).map((k) => (
                                        <option key={k} value={k}>
                                          {k}
                                        </option>
                                      ))}
                                    </StudioSelect>
                                  </Field>
                                )}
                                <Field
                                  label="已知商品信息"
                                  hint="尺寸、材质、卖点等以真实商品为准。"
                                >
                                  <textarea
                                    className="kv-textarea custom-scrollbar"
                                    rows={2}
                                    disabled={busy}
                                    value={p.facts}
                                    onChange={(e) =>
                                      patchProduct(p.id, {
                                        facts: e.target.value,
                                      })
                                    }
                                    placeholder="例如：容量 20L，尼龙面料，有独立电脑隔层"
                                  />
                                </Field>
                              </article>
                            )
                          })}
                        </div>
                      )}
                      {brief.products.length > 0 && (
                        <Button
                          size="sm"
                          disabled={busy}
                          onClick={() => void act({ kind: 'classify' })}
                        >
                          <ScanLine size={15} />让 Agent 识别商品
                          {brief.feature === 'client' ? '并分类' : ''}
                        </Button>
                      )}
                    </section>
                    <section className="is-section is-requirements-section">
                      <div className="is-section-heading">
                        <div>
                          <span className="is-step-number">02</span>
                          <h3>这次要做什么</h3>
                        </div>
                      </div>
                      {brief.feature === 'gen' && (
                        <div className="is-preset-chips">
                          {SHOTS.slice(0, 5).map((s) => (
                            <Button
                              size="sm"
                              disabled={busy}
                              key={s}
                              onClick={() =>
                                patch({
                                  requirement: `${s}。${brief.requirement}`,
                                })
                              }
                            >
                              {s}
                            </Button>
                          ))}
                          <details>
                            <summary>更多场景</summary>
                            <div>
                              {SHOTS.slice(5).map((s) => (
                                <Button
                                  size="sm"
                                  disabled={busy}
                                  key={s}
                                  onClick={() =>
                                    patch({
                                      requirement: `${s}。${brief.requirement}`,
                                    })
                                  }
                                >
                                  {s}
                                </Button>
                              ))}
                            </div>
                          </details>
                        </div>
                      )}
                      <Field label="图片要求">
                        <textarea
                          className="kv-textarea custom-scrollbar"
                          rows={5}
                          disabled={busy}
                          value={brief.requirement}
                          onChange={(e) => patch({ requirement: e.target.value })}
                          placeholder="例如：为这款背包做一套巴西市场商品图，突出大容量和通勤场景。画面简洁，保留商品原本的颜色和 Logo。"
                        />
                      </Field>
                      <Field label="统一风格（可选）">
                        <textarea
                          className="kv-textarea custom-scrollbar"
                          rows={2}
                          disabled={busy}
                          value={brief.style}
                          onChange={(e) => patch({ style: e.target.value })}
                          placeholder="色调、背景、光线、文字风格；填写后优先于模板默认风格"
                        />
                      </Field>
                    </section>
                  </div>
                  <aside className="is-spec-column">
                    <section className="is-section is-spec-section">
                      <div className="is-section-heading">
                        <div>
                          <Settings2 size={16} />
                          <h3>创作规格</h3>
                        </div>
                      </div>
                      <Field label="任务名称">
                        <input
                          className="kv-input"
                          disabled={busy}
                          value={brief.name}
                          onChange={(e) => patch({ name: e.target.value })}
                          placeholder="例如：秋季背包 · 巴西主图"
                        />
                      </Field>
                      {['replace', 'smart', 'client'].includes(brief.feature) && (
                        <Field label="使用模板">
                          <StudioSelect
                            disabled={busy}
                            value={brief.templateId || ''}
                            onChange={(e) => {
                              const t = templates.find((t) => t.id === e.target.value)
                              patch({
                                templateId: t?.id || null,
                                count: t?.data.slots.length || brief.count,
                                language: t?.data.language || brief.language,
                                ratio: t?.data.output?.ratio || brief.ratio,
                                resolution: t?.data.output?.resolution || brief.resolution,
                              })
                            }}
                          >
                            <option value="">选择模板</option>
                            {templates
                              .filter(
                                (t) =>
                                  brief.feature === 'client' ||
                                  t.data.mode ===
                                    (brief.feature === 'replace' ? 'replace' : 'smart'),
                              )
                              .map((t) => (
                                <option key={t.id} value={t.id}>
                                  {t.data.name}
                                </option>
                              ))}
                          </StudioSelect>
                          {template && (
                            <small>
                              {template.data.slots.length} 页 · {template.data.category}
                            </small>
                          )}
                          {brief.feature === 'replace' &&
                            !templates.some((t) => t.data.mode === 'replace') && (
                              <Button size="sm" onClick={() => setView('templates')}>
                                去模板库导入样图模板
                              </Button>
                            )}
                        </Field>
                      )}
                      <Field label="使用平台">
                        <StudioSelect
                          disabled={busy}
                          value={brief.platform}
                          onChange={(e) => patch({ platform: e.target.value })}
                        >
                          {[
                            '通用电商',
                            'Amazon',
                            'Shopify',
                            'Mercado Livre',
                            'Shopee',
                            'TikTok Shop',
                            '社交媒体 / 广告',
                          ].map((s) => (
                            <option key={s}>{s}</option>
                          ))}
                        </StudioSelect>
                      </Field>
                      <Field label="图内语言">
                        <ImageLanguageSelect
                          disabled={busy}
                          value={brief.language}
                          onChange={(language) => patch({ language })}
                        />
                      </Field>
                      <div className="is-field">
                        <span>画幅</span>
                        <div className="is-ratios" role="group" aria-label="画幅">
                          {['1:1', '2:3', '3:2', '3:4', '4:3', '4:5', '5:4', '9:16', '16:9'].map(
                            (r) => (
                              <button
                                type="button"
                                key={r}
                                disabled={busy}
                                aria-pressed={brief.ratio === r}
                                aria-label={r}
                                className={brief.ratio === r ? 'active' : ''}
                                onClick={() => patch({ ratio: r })}
                              >
                                <span style={{ aspectRatio: r.replace(':', '/') }} />
                                {r}
                              </button>
                            ),
                          )}
                        </div>
                      </div>
                      <div className="is-two-cols">
                        <Field label="生成清晰度">
                          <StudioSelect
                            disabled={busy}
                            value={brief.resolution}
                            onChange={(e) => patch({ resolution: e.target.value })}
                          >
                            <option value="1k">标准 / 1K</option>
                            <option value="2k">2K</option>
                            <option value="4k">4K</option>
                          </StudioSelect>
                        </Field>
                        <Field label="每款张数">
                          <input
                            className="kv-input"
                            type="number"
                            min={1}
                            max={30}
                            disabled={busy || !!template}
                            value={template?.data.slots.length || brief.count}
                            onChange={(e) =>
                              patch({
                                count: Math.max(1, Math.min(30, Number(e.target.value))),
                              })
                            }
                          />
                        </Field>
                      </div>
                      <p className="is-muted is-small">
                        实际支持的画幅和清晰度由图片接口决定。原图保留，交付尺寸在导出时设置。
                      </p>
                    </section>
                  </aside>
                  <div className="is-tip">
                    <CheckCircle2 size={16} />
                    <p>
                      {brief.feature === 'gen'
                        ? '原图和每一次修改都会保留，随时可以对比不同版本。'
                        : '先做两款样品。商品不足两款时全部作为样品，每个分类单独确认。'}
                    </p>
                  </div>
                  <div className="is-creation-footer">
                    <div>
                      <Sparkles size={17} />
                      <span>
                        Agent 负责看图和规划
                        <br />
                        <small>方案可编辑，确认后再生图</small>
                      </span>
                    </div>
                    <Button
                      variant="primary"
                      disabled={busy || loading}
                      onClick={() => void act({ kind: 'plan' })}
                    >
                      {busy ? (
                        <Loader2 size={16} className="is-spinning" />
                      ) : (
                        <Sparkles size={16} />
                      )}
                      生成画面方案
                      <ArrowRight size={15} />
                    </Button>
                  </div>
                </div>
              )}
              {stage === 'plan' && (
                <section className="is-plan-page">
                  <div className="is-section-heading">
                    <div>
                      <h3>画面方案</h3>
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
                            <Field label="完整画面提示词（包括需要修改的图内文案）">
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
                            </Field>
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
                      <span className="is-count">{successCount} 张</span>
                    </div>
                    <div className="is-actions">
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
                  {brief.feature !== 'gen' && task && (
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
                        {approved ? '规划剩余商品并批量出图' : '样品通过，允许批量'}
                        <ArrowRight size={14} />
                      </Button>
                    </div>
                  )}
                  {!results.length ? (
                    <EmptyState
                      icon={<ImageIcon size={36} />}
                      title={running ? '正在把方案变成图片' : '你的成图会出现在这里'}
                      text={
                        running
                          ? '每完成一张就会保存一张，可以切换到其他页面。'
                          : '先生成画面方案，再开始出图。成图支持逐张修改和质检。'
                      }
                      action={
                        <Button disabled={busy} onClick={() => setStage('plan')}>
                          查看画面方案
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
                            className={`is-result-card ${r.path ? '' : 'failed'}`}
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
                                  <p>{r.remoteId ? '远程任务已保存' : '等待图片结果'}</p>
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
        </main>
      </div>
      {settingsOpen && (
        <ConfigPanel
          config={config}
          providers={providers}
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
                  disabled={busy || dirty}
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
              <div className="is-divider" />
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
                冻结为样图模板
              </Button>
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
                    const dest = await open({
                      directory: true,
                      title: '选择交付图保存位置',
                    })
                    if (typeof dest === 'string') {
                      const path = await api.imageStudioExport(
                        task.id,
                        dest,
                        delivery.width,
                        delivery.height,
                        delivery.maxKb,
                      )
                      setExportOpen(false)
                      setNotice(`已导出到 ${path}`)
                    }
                  })
                }
              >
                <Download size={15} />
                选择位置并导出
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
