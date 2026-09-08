import { useEffect, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import {
  Clapperboard,
  Film,
  FolderOpen,
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
import './VideoStudio.css'
import { VideoMediaOptions } from './VideoMediaOptions'

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
  const [data, setData] = useState<VideoBootstrap>(preview)
  const [view, setView] = useState<
    'creation' | 'analysis' | 'templates' | 'settings'
  >('creation')
  const [brief, setBrief] = useState<VideoBrief>(newVideoBrief)
  const [task, setTask] = useState<VideoTask>()
  const [script, setScript] = useState('')
  const [dirty, setDirty] = useState(false)
  const [busy, setBusy] = useState('')
  const [error, setError] = useState('')
  const [step, setStep] = useState(0)
  const [templateName, setTemplateName] = useState('')
  const [provider, setProvider] = useState('comfy')
  const [base, setBase] = useState('http://127.0.0.1:8188')
  const [key, setKey] = useState('')
  const [model, setModel] = useState('grok-imagine-video-1.5')
  const [video, setVideo] = useState('')
  const [recoveryId, setRecoveryId] = useState('')
  const locked =
    !!task &&
    ['submitting', 'running', 'succeeded', 'uncertain'].includes(task.status)

  async function refresh() {
    if (native) setData(await api.videoStudioBootstrap())
  }
  useEffect(() => {
    if (native)
      void api
        .videoStudioBootstrap()
        .then(setData)
        .catch((e) => setError(String(e)))
  }, [native])
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
      if (native)
        void api
          .videoStudioBootstrap()
          .then(setData)
          .catch(() => {})
    }
    window.addEventListener('focus', focus)
    return () => window.removeEventListener('focus', focus)
  }, [native])

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
  function fresh(mode: VideoBrief['mode'], template?: VideoTemplate) {
    setTask(undefined)
    setBrief({
      ...newVideoBrief(mode),
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
    setView(mode)
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
  async function saved() {
    let t = task
    if (!t) t = await api.videoStudioTask('create', { brief })
    if (dirty)
      t = await api.videoStudioTask('save', {
        id: t.id,
        revision: t.revision,
        brief,
        script,
      })
    accept(t)
    return t
  }
  async function run(action: string) {
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
          action === 'get' && task
            ? await api.videoStudioTask('get', { id: task.id })
            : await saved()
        if (action !== 'save' && action !== 'get') {
          t = await api.videoStudioTask(action, {
            id: t.id,
            revision: t.revision,
            confirmSpend: action === 'submit',
          })
          accept(t)
          if (action === 'approve')
            t = await api.videoStudioTask('prepare', {
              id: t.id,
              revision: t.revision,
            })
        }
        accept(t)
        if (action === 'plan' || action === 'analyze') setStep(1)
        if (action === 'approve' || action === 'submit') setStep(2)
      },
    )
  }
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
    <div className="kv image-studio video-studio">
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
                setView('creation')
                if (brief.mode !== 'creation') fresh('creation')
              }}
            >
              <Clapperboard size={17} />
              <span>视频创作</span>
            </button>
            <button
              className={view === 'analysis' ? 'active' : ''}
              disabled={!!busy}
              onClick={() => {
                setView('analysis')
                if (brief.mode !== 'analysis') fresh('analysis')
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
          </nav>
          <div className="is-rail-history custom-scrollbar">
            <div className="is-rail-label">最近任务</div>
            {!data.tasks.length && (
              <p className="vs-muted">剧本、成片和远程任务会保存在这里。</p>
            )}
            {data.tasks.map((t) => (
              <button
                key={t.id}
                disabled={!!busy}
                className={t.id === task?.id ? 'active' : ''}
                onClick={() =>
                  void guarded('打开任务…', async () => {
                    const latest = await api.videoStudioTask('get', {
                      id: t.id,
                    })
                    accept(latest)
                    setView(latest.brief.mode)
                    setStep(latest.prompt ? 2 : latest.script ? 1 : 0)
                  })
                }
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
          {error && (
            <div role="alert" className="vs-error">
              {error}
            </div>
          )}
          {busy && (
            <div role="status" className="vs-notice">
              <RefreshCw size={14} className="vs-spin" />
              {busy}
            </div>
          )}
          {view === 'settings' ? (
            <>
              <div className="vs-heading">
                <h2>视频设置</h2>
                <span className="vs-muted">聊天与视频页面共用</span>
              </div>
              <section className="vs-panel vs-settings">
                <Field label="配置路线">
                  <StudioSelect
                    value={provider}
                    onChange={(e) => {
                      const p = e.target.value
                      setProvider(p)
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
                    onChange={(e) => setBase(e.target.value)}
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
                      onChange={(e) => setKey(e.target.value)}
                    />
                  </Field>
                )}
                {provider === 'grok' && (
                  <Field label="模型">
                    <input
                      className="kv-input"
                      value={model}
                      onChange={(e) => setModel(e.target.value)}
                    />
                  </Field>
                )}
                <Button
                  variant="primary"
                  disabled={!native || !!busy}
                  onClick={() =>
                    void guarded('保存配置…', async () => {
                      const config = await api.videoStudioConfig({
                        name: provider,
                        base_url: base,
                        api_key: key,
                        model,
                      })
                      setData((d) => ({ ...d, config }))
                      setKey('')
                    })
                  }
                >
                  保存配置
                </Button>
              </section>
              <section className="vs-panel">
                <h3>运行环境</h3>
                <p className="vs-muted">
                  导演使用应用当前聊天模型，处理图片需模型支持视觉。
                </p>
                <dl className="vs-specs">
                  <dt>Python</dt>
                  <dd>{data.dependencies.python || '未检测'}</dd>
                  <dt>Comfy MCP</dt>
                  <dd>{data.dependencies.comfy ? '已找到' : '未找到'}</dd>
                  <dt>Node.js</dt>
                  <dd>
                    {data.dependencies.node
                      ? '已找到（分析需 ≥ 22.12）'
                      : '未找到'}
                  </dd>
                  <dt>FFmpeg</dt>
                  <dd>{data.dependencies.ffmpeg ? '已找到' : '未找到'}</dd>
                </dl>
                <p className="vs-muted">
                  ComfyUI 路线需要 comfy-mcp 0.10.0、comfy-cli ≥ 1.14 和服务端
                  H3 工作流节点。参考视频分析首次运行由 npx 加载固定版本
                  MCP；部分平台还需 yt-dlp。
                </p>
                <Button
                  size="sm"
                  disabled={!native || !!busy}
                  onClick={() => void guarded('检查环境…', refresh)}
                >
                  重新检查
                </Button>
                <Button
                  size="sm"
                  disabled={!native || !!busy}
                  onClick={() =>
                    void guarded(
                      '正在安装 ComfyUI 客户端依赖，可能需要几分钟…',
                      async () => {
                        await api.videoStudioInstallComfy()
                        await refresh()
                      },
                    )
                  }
                >
                  安装 / 修复 Comfy 依赖
                </Button>
                <p className="vs-muted">
                  依赖安装到应用自己的运行目录，不修改系统 Python 包。
                </p>
                <p className="vs-path">{data.configPath}</p>
              </section>
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
                        <pre>
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
                <h2>{view === 'analysis' ? '参考视频分析' : '视频创作'}</h2>
                <div className="vs-actions">
                  <span className="vs-muted">
                    {task ? videoStatus[task.status] || task.status : '新任务'}
                    {dirty ? ' · 未保存' : ''}
                  </span>
                  <Button
                    size="sm"
                    disabled={!!busy}
                    onClick={() => fresh(view)}
                  >
                    <Plus size={14} />
                    新建
                  </Button>
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
              <div className="is-work-toolbar">
                <div
                  className="is-stage-tabs"
                  role="tablist"
                  aria-label="视频制作步骤"
                >
                  {(view === 'analysis'
                    ? ['参考素材', '拆解结果']
                    : ['素材与要求', '导演剧本', '生成与成片']
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
              </div>
              {step === 0 ? (
                <div className="vs-layout">
                  <div>
                    <section className="vs-panel">
                      <h3>{view === 'analysis' ? '参考视频' : '商品素材'}</h3>
                      {view === 'analysis' ? (
                        <>
                          <Field
                            label="视频链接或本地路径"
                            hint="支持单条视频。分析不会提交任何生成任务。"
                          >
                            <input
                              className="kv-input"
                              disabled={controlsDisabled}
                              placeholder="粘贴视频链接，或选择本地文件"
                              value={brief.source}
                              onChange={(e) =>
                                change({ source: e.target.value })
                              }
                            />
                          </Field>
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
                            选择本地视频
                          </Button>
                        </>
                      ) : (
                        <>
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
                          {!brief.images.length && (
                            <div className="vs-empty">
                              <Film size={30} />
                              <p>添加商品图片，让视频保持商品原貌</p>
                              <small>也可以仅用文字创作视频</small>
                            </div>
                          )}
                          <Button
                            size="sm"
                            disabled={!native || controlsDisabled}
                            onClick={() => void pickImages()}
                          >
                            <Plus size={14} />
                            添加参考图
                          </Button>
                          <p className="vs-muted">
                            Grok 单图 1 张 / 参考图最多 7 张 · ComfyUI 最多 3 张
                            · MiniMax 最多 9 张
                          </p>
                        </>
                      )}
                    </section>
                    <section className="vs-panel">
                      <Field
                        label={
                          view === 'analysis' ? '重点分析什么' : '这次要拍什么'
                        }
                      >
                        <textarea
                          className="kv-textarea"
                          disabled={controlsDisabled}
                          rows={6}
                          placeholder="商品展示、使用动作、场景、镜头节奏、口播与结尾要求…"
                          value={brief.request}
                          onChange={(e) => change({ request: e.target.value })}
                        />
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
                      <div className="vs-actions">
                        <Button
                          variant="primary"
                          disabled={
                            !native ||
                            controlsDisabled ||
                            (view === 'analysis'
                              ? !brief.source.trim()
                              : !brief.request.trim() && !brief.template)
                          }
                          onClick={() =>
                            void run(view === 'analysis' ? 'analyze' : 'plan')
                          }
                        >
                          <WandSparkles size={15} />
                          {view === 'analysis' ? '开始拆解' : '生成导演剧本'}
                        </Button>
                        <Button
                          disabled={!native || controlsDisabled}
                          onClick={() => void run('save')}
                        >
                          保存草稿
                        </Button>
                      </div>
                    </section>
                  </div>
                  <section className="vs-panel vs-options">
                    <Field label="任务名称">
                      <input
                        className="kv-input"
                        disabled={controlsDisabled}
                        placeholder="例如：商务背包展示"
                        value={brief.name}
                        onChange={(e) => change({ name: e.target.value })}
                      />
                    </Field>
                    <Field label="口播 / 文案语言">
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
                    </Field>
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
                          label="生成路线"
                          hint="先明确选择路线，生成失败不会自动切换。"
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
                        <VideoMediaOptions
                          brief={brief}
                          change={change}
                          disabled={controlsDisabled}
                          native={native}
                          onError={setError}
                        />
                        <Field
                          label={
                            route === 'comfy' ? '工作流百万像素' : '生成清晰度'
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
                                {v === 'adaptive' ? '自适应' : v}
                              </option>
                            ))}
                          </StudioSelect>
                        </Field>
                      </>
                    )}
                  </section>
                </div>
              ) : step === 1 ? (
                <>
                  <section className="vs-panel">
                    <div className="vs-heading">
                      <h3>{view === 'analysis' ? '逐镜头拆解' : '导演剧本'}</h3>
                      <small className="vs-muted">
                        {task?.approved && !dirty
                          ? '已确认此版本'
                          : '可编辑 · 修改后需重新确认'}
                      </small>
                    </div>
                    <textarea
                      className="kv-textarea vs-script custom-scrollbar"
                      disabled={controlsDisabled}
                      value={script}
                      onChange={(e) => {
                        setScript(e.target.value)
                        setDirty(true)
                      }}
                      placeholder="生成剧本后在这里查看镜头、动作、声音和时间线。也可以直接填写已有剧本。"
                    />
                    <div className="vs-actions">
                      <Button
                        disabled={!native || controlsDisabled}
                        onClick={() => void run('save')}
                      >
                        保存修改
                      </Button>
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
                          确认剧本并转换提示词
                        </Button>
                      )}
                    </div>
                    {view === 'creation' && (!route || !brief.resolution) && (
                      <p className="vs-muted">
                        请先在「素材与要求」中选择生成路线和清晰度。
                      </p>
                    )}
                  </section>
                  {view === 'analysis' && (
                    <section className="vs-panel">
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
                    <details>
                      <summary>查看转换后的提示词</summary>
                      <pre>
                        {dirty
                          ? '内容已修改，请重新确认剧本。'
                          : task?.prompt || '请先确认剧本并转换提示词。'}
                      </pre>
                    </details>
                    {task?.quote && !dirty && (
                      <div className="vs-quote">
                        <strong>
                          {task.quote.currency && task.quote.estimated_cost
                            ? `预计 ${task.quote.estimated_cost[brief.resolution]} ${task.quote.currency}`
                            : 'ComfyUI 工作流执行'}
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
                    {!locked && (
                      <div className="vs-actions">
                        <Button
                          disabled={!native || !!busy || dirty || !task?.prompt}
                          onClick={() => void run('quote')}
                        >
                          查询费用
                        </Button>
                        <Button
                          variant="primary"
                          disabled={
                            !native ||
                            !!busy ||
                            dirty ||
                            !task?.quote ||
                            !task.prompt
                          }
                          onClick={() => void run('submit')}
                        >
                          确认费用并生成
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
                      <p className="vs-path">
                        任务编号：{task.remote.id || '尚未获得可靠编号'}
                      </p>
                      {task.error && <p role="alert">{task.error}</p>}
                      {task.remote.id && task.status !== 'succeeded' && (
                        <Button
                          disabled={!!busy}
                          onClick={() => void run('poll')}
                        >
                          <RefreshCw size={14} />
                          查询进度 / 恢复结果
                        </Button>
                      )}
                      <p className="vs-muted">
                        已提交任务保留原服务地址和编号。重新打开后可继续查询。
                      </p>
                    </section>
                  )}
                  {task &&
                    ['uncertain', 'submitting'].includes(task.status) && (
                      <section className="vs-panel">
                        <Field
                          label="补录远程任务编号"
                          hint="到原供应商控制台或 ComfyUI 队列核实该任务，再粘贴编号恢复查询。"
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
                      </section>
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
