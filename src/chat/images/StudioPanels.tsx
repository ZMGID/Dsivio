import {
  Children,
  cloneElement,
  isValidElement,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { Check, FolderOpen, Image as ImageIcon, Plus, Save, Settings2, X } from 'lucide-react'
import { api, isTauriRuntime, type ModelProvider } from '../../api/tauri'
import { getSettingsCached } from '../../api/settingsCache'
import { Button, IconButton } from '../../components/Button'
import { Select } from '../../settings/components'
import { ModelPairSelect } from '../../settings/ModelPairSelect'
import type { ImageConfig, ImageTemplate, ImageTemplateSlot } from './types'
import { builtinImageUrl } from './builtinTemplates'
import { inferImageStudioProtocol, isImageGenerationModel, isVisionModel } from './studioModels'

/** Adapt option children to the application's shared Select; no separate menu styling. */
export function StudioSelect({
  value,
  onChange,
  children,
  disabled,
  ariaLabel,
}: {
  value: string | number
  onChange: (event: { target: { value: string } }) => void
  children: ReactNode
  disabled?: boolean
  ariaLabel?: string
}) {
  const text = (node: ReactNode): string =>
    Children.toArray(node)
      .map((child) =>
        isValidElement<{ children?: ReactNode }>(child)
          ? text(child.props.children)
          : String(child),
      )
      .join('')
  const options = Children.toArray(children).flatMap((child) => {
    if (!isValidElement<{ value?: string; children?: ReactNode }>(child)) return []
    const label = text(child.props.children)
    return [{ value: child.props.value ?? label, label }]
  })
  return (
    <Select
      value={String(value)}
      onChange={(value) => onChange({ target: { value } })}
      options={options}
      disabled={disabled}
      ariaLabel={ariaLabel}
    />
  )
}

const imageLanguages = [
  ['pt-BR', '葡萄牙语（巴西）'],
  ['en-US', '英语（美国）'],
  ['en-GB', '英语（英国）'],
  ['es', '西班牙语'],
  ['zh-CN', '简体中文'],
  ['zh-TW', '繁体中文'],
  ['ja', '日语'],
  ['ko', '韩语'],
  ['fr', '法语'],
  ['de', '德语'],
  ['it', '意大利语'],
  ['ar', '阿拉伯语'],
  ['id', '印尼语'],
  ['th', '泰语'],
  ['vi', '越南语'],
  ['无文字', '无文字'],
]

export function ImageLanguageSelect({
  value,
  onChange,
  disabled,
}: {
  value: string
  onChange: (value: string) => void
  disabled?: boolean
}) {
  const known = imageLanguages.some(([code]) => code === value)
  return (
    <div className="is-field">
      <StudioSelect
        ariaLabel="图内语言"
        disabled={disabled}
        value={known ? value : 'custom'}
        onChange={(e) => onChange(e.target.value === 'custom' ? '' : e.target.value)}
      >
        {imageLanguages.map(([code, label]) => (
          <option key={code} value={code}>
            {label}
          </option>
        ))}
        <option value="custom">其他语言 / 自定义说明…</option>
      </StudioSelect>
      {!known && (
        <input
          className="kv-input"
          aria-label="自定义图内语言"
          placeholder="例如：西班牙语（墨西哥），保留品牌英文"
          disabled={disabled}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </div>
  )
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: ReactNode
}) {
  return (
    <label className="is-field">
      <span>{label}</span>
      {Children.map(children, (child) =>
        isValidElement<{ ariaLabel?: string }>(child) && child.type === StudioSelect
          ? cloneElement(child, { ariaLabel: label })
          : child,
      )}
      {hint && <small>{hint}</small>}
    </label>
  )
}

export function AssetImage({
  path,
  name,
  large = false,
  fallback,
}: {
  path: string
  name: string
  large?: boolean
  fallback?: ReactNode
}) {
  const [url, setUrl] = useState('')
  const [error, setError] = useState('')
  const bundledUrl = builtinImageUrl(path)
  useEffect(() => {
    let alive = true
    setUrl('')
    setError('')
    if (isTauriRuntime() && !bundledUrl)
      void api
        .imageStudioPreview(path, large)
        .then((u) => {
          if (alive) setUrl(u)
        })
        .catch((e) => {
          if (alive) setError(String(e))
        })
    return () => {
      alive = false
    }
  }, [path, large, bundledUrl])
  return (bundledUrl || url) && !error ? (
    <img src={bundledUrl || url} alt={name} loading="lazy" onError={() => setError('预览失败')} />
  ) : (
    (fallback ?? (
      <span className="is-image-placeholder" title={error || name}>
        <ImageIcon size={24} />
        {error && <small>预览失败</small>}
      </span>
    ))
  )
}

export function ConfigPanel({
  config,
  onSave,
  onClose,
}: {
  config: ImageConfig
  onSave: (c: ImageConfig) => Promise<void>
  onClose: () => void
}) {
  const [draft, setDraft] = useState(config)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const [providers, setProviders] = useState<ModelProvider[]>([])
  useEffect(() => {
    let alive = true
    void getSettingsCached()
      .then((settings) => {
        if (alive) setProviders(settings.providers || [])
      })
      .catch(() => {
        if (alive) setProviders([])
      })
    return () => {
      alive = false
    }
  }, [])
  return (
    <div className="kv-modal-backdrop kv-modal-backdrop--portal is-overlay" onClick={onClose}>
      <section
        className="kv-modal is-dialog custom-scrollbar"
        role="dialog"
        aria-modal="true"
        aria-label="图片设置"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="is-section-heading">
          <div>
            <Settings2 size={18} />
            <h2>图片设置</h2>
          </div>
          <IconButton label="关闭设置" onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        <p className="is-muted">
          配置一次，所有图片功能共用。只列出已标记生图能力的模型；密钥和适配器沿用「设置 → 模型」。
        </p>
        <Field label="图片保存位置">
          <div className="is-actions">
            <input className="kv-input" readOnly value={draft.outputRoot || '系统图片目录 / Dsivio / Images'} />
            <IconButton label="选择图片保存位置" disabled={pending} onClick={() => {
              void open({ directory: true, title: '选择新任务的图片保存位置' }).then((path) => {
                if (typeof path === 'string') setDraft((d) => ({ ...d, outputRoot: path }))
              }).catch((e) => setError(String(e)))
            }}><FolderOpen size={16} /></IconButton>
          </div>
          <small>按任务分文件夹，原图自动保存。更换位置只影响新任务。</small>
        </Field>
        <div className="is-field">
          <span>图片模型</span>
          <ModelPairSelect
            className="w-full"
            ariaLabel="图片模型"
            providerId={draft.providerId}
            model={draft.model}
            providers={providers}
            inheritLabel="选择生图模型"
            filterModel={isImageGenerationModel}
            onChange={(providerId, model) =>
              setDraft((d) => ({
                ...d,
                providerId,
                model,
                protocol: inferImageStudioProtocol(
                  providers.find((p) => p.id === providerId),
                  model,
                ),
              }))
            }
          />
          <small>仅显示模型库 / 适配器标记为生图的模型。没有生图模型的供应商不会出现。</small>
        </div>
        <div className="is-divider" />
        <div className="is-field">
          <span>Agent 模型</span>
          <ModelPairSelect
            className="w-full"
            ariaLabel="Agent 模型"
            providerId={draft.agentProviderId}
            model={draft.agentModel}
            providers={providers}
            inheritLabel="使用当前聊天模型"
            filterModel={isVisionModel}
            onChange={(agentProviderId, agentModel) =>
              setDraft((d) => ({ ...d, agentProviderId, agentModel }))
            }
          />
          <small>用于商品识别、画面规划和质检，请选择支持看图的模型。</small>
        </div>
        {error && (
          <p role="alert" className="is-error">
            {error}
          </p>
        )}
        <div className="is-dialog-actions">
          <Button onClick={onClose}>取消</Button>
          <Button
            variant="primary"
            disabled={pending}
            onClick={() => {
              setPending(true)
              const next = {
                ...draft,
                protocol: inferImageStudioProtocol(
                  providers.find((p) => p.id === draft.providerId),
                  draft.model,
                ),
              }
              void onSave(next)
                .catch((e) => setError(String(e)))
                .finally(() => setPending(false))
            }}
          >
            <Check size={15} />
            保存配置
          </Button>
        </div>
      </section>
    </div>
  )
}

const blankTemplate = (): ImageTemplate => ({
  id: '',
  directory: '',
  builtin: false,
  data: {
    name: '新套图模板',
    mode: 'smart',
    category: '通用电商',
    language: 'pt-BR',
    style: '',
    output: { ratio: '1:1', resolution: '1k' },
    slots: [
      {
        id: 'h1',
        purpose: '主视觉',
        brief: '用一张图展示商品整体外观和最重要的卖点。',
      },
    ],
  },
})

const slotDescription = (slot: ImageTemplateSlot) =>
  slot.brief?.trim() || slot.prompt?.trim() || '此页未填写画面说明，可在「查看 / 编辑」中补充。'

function TemplatePagePreview({
  template,
  initialIndex,
  onClose,
}: {
  template: ImageTemplate
  initialIndex: number
  onClose: () => void
}) {
  const [index, setIndex] = useState(initialIndex)
  const dialogRef = useRef<HTMLElement>(null)
  const slot = template.data.slots[index]
  const example = slot.example && template.directory ? `${template.directory}/${slot.example}` : ''

  useEffect(() => {
    const trigger = document.activeElement
    dialogRef.current?.focus()
    return () => {
      if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus()
    }
  }, [])

  return (
    <div
      className="kv-modal-backdrop kv-modal-backdrop--portal is-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <section
        ref={dialogRef}
        className="kv-modal is-dialog is-template-preview-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`${template.data.name} · 页面预览`}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.stopPropagation()
            onClose()
          }
          if (e.key === 'Tab') {
            const buttons =
              dialogRef.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')
            if (!buttons?.length) return
            const first = buttons[0]
            const last = buttons[buttons.length - 1]
            if (e.shiftKey && document.activeElement === first) {
              e.preventDefault()
              last.focus()
            } else if (!e.shiftKey && document.activeElement === last) {
              e.preventDefault()
              first.focus()
            }
          }
        }}
      >
        <div className="is-section-heading">
          <div>
            <h2>{template.data.name}</h2>
          </div>
          <IconButton label="关闭页面预览" onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        <div className="is-template-preview-body custom-scrollbar" key={index}>
          <p className="is-muted">
            第 {index + 1} / {template.data.slots.length} 张 · {slot.id}
          </p>
          <h3>{slot.purpose || `第 ${index + 1} 张`}</h3>
          {example && (
            <div className="is-template-preview-image">
              <AssetImage
                path={example}
                name={`${template.data.name} · ${slot.purpose || slot.id}参考图`}
                large
                fallback={<p className="is-muted">参考图暂不可用，可先查看下方内容说明。</p>}
              />
            </div>
          )}
          <div className="is-template-preview-description">
            <h4>{example ? '内容说明' : '内容概要 · 暂无参考图'}</h4>
            <p>{slotDescription(slot)}</p>
          </div>
        </div>
        <div className="is-dialog-actions">
          <Button disabled={index === 0} onClick={() => setIndex((i) => i - 1)}>
            上一张
          </Button>
          <Button
            disabled={index === template.data.slots.length - 1}
            onClick={() => setIndex((i) => i + 1)}
          >
            下一张
          </Button>
        </div>
      </section>
    </div>
  )
}

export function TemplatePanel({
  templates,
  onChange,
  onUse,
  report,
}: {
  templates: ImageTemplate[]
  onChange: (t: ImageTemplate) => void
  onUse: (t: ImageTemplate) => void
  report: (e: unknown) => void
}) {
  const [editing, setEditing] = useState<ImageTemplate | null>(null)
  const [preview, setPreview] = useState<{
    template: ImageTemplate
    index: number
  } | null>(null)
  const [pending, setPending] = useState(false)
  const perform = async (fn: () => Promise<void>) => {
    if (!isTauriRuntime()) {
      report('请在 Dsivio 桌面窗口中导入、保存或导出模板')
      return
    }
    setPending(true)
    try {
      await fn()
    } catch (e) {
      report(e)
    } finally {
      setPending(false)
    }
  }
  const editData = (patch: Partial<ImageTemplate['data']>) =>
    setEditing((t) => (t ? { ...t, data: { ...t.data, ...patch } } : t))
  return (
    <div className="is-template-page">
      <div className="is-section-heading">
        <div>
          <h2>模板库</h2>
          <span className="is-count">{templates.length}</span>
        </div>
        <div className="is-actions">
          <Button
            disabled={pending}
            onClick={() =>
              void perform(async () => {
                const path = await open({
                  directory: true,
                  title: '选择包含 template.json 和示例图的文件夹',
                })
                if (typeof path === 'string') onChange(await api.imageStudioTemplateImport(path))
              })
            }
          >
            <FolderOpen size={15} />
            导入模板文件夹
          </Button>
          <Button variant="primary" onClick={() => setEditing(blankTemplate())}>
            <Plus size={15} />
            创建规则模板
          </Button>
        </div>
      </div>
      <p className="is-muted">
        预览前 3 张，有参考图时显示参考图，否则显示内容概要。点击预览可查看整套内容。
      </p>
      <div className="is-template-grid">
        {templates.map((t) => (
          <article className="is-template-card" key={t.id}>
            <div className="is-template-cover">
              <div className="is-template-cover-heading">
                <span>{t.data.mode === 'replace' ? '样图换货' : '风格规则'}</span>
                <span>共 {t.data.slots.length} 张</span>
              </div>
              <div className="is-template-pages">
                {t.data.slots.slice(0, 3).map((slot, index) => {
                  const purpose = slot.purpose || `第 ${index + 1} 张`
                  const summary = (
                    <span className="is-template-slot-summary">
                      <small>内容概要</small>
                      <span>{slotDescription(slot)}</span>
                    </span>
                  )
                  return (
                    <Button
                      key={slot.id}
                      className="is-template-slot"
                      aria-label={`${t.data.name} · 第 ${index + 1} 张：${purpose}`}
                      onClick={() => setPreview({ template: t, index })}
                    >
                      <span className="is-template-slot-visual">
                        {slot.example && t.directory ? (
                          <AssetImage
                            path={`${t.directory}/${slot.example}`}
                            name={`${purpose}参考图`}
                            fallback={summary}
                          />
                        ) : (
                          summary
                        )}
                      </span>
                      <span className="is-template-slot-caption">
                        <span>{String(index + 1).padStart(2, '0')}</span>
                        <strong>{purpose}</strong>
                      </span>
                    </Button>
                  )
                })}
              </div>
            </div>
            <div className="is-template-info">
              <h3>
                {t.data.name}
                {t.builtin && <small>内置</small>}
              </h3>
              <p>{t.data.category || '通用电商'}</p>
              <span className="is-muted">
                {t.data.output?.ratio || '1:1'} · {t.data.language?.split('。')[0] || '跟随任务'}
              </span>
              <div className="is-actions">
                <Button variant="primary" size="sm" onClick={() => onUse(t)}>
                  使用模板
                </Button>
                <Button size="sm" onClick={() => setEditing(structuredClone(t))}>
                  查看 / 编辑
                </Button>
                <Button
                  size="sm"
                  disabled={pending}
                  onClick={() =>
                    void perform(async () => {
                      const dest = await open({
                        directory: true,
                        title: '选择模板导出位置',
                      })
                      if (typeof dest === 'string') await api.imageStudioTemplateExport(t.id, dest)
                    })
                  }
                >
                  导出
                </Button>
              </div>
            </div>
          </article>
        ))}
      </div>
      {preview && (
        <TemplatePagePreview
          template={preview.template}
          initialIndex={preview.index}
          onClose={() => setPreview(null)}
        />
      )}
      {editing && (
        <div className="kv-modal-backdrop kv-modal-backdrop--portal is-overlay">
          <section
            className="kv-modal is-dialog is-template-editor custom-scrollbar"
            role="dialog"
            aria-modal="true"
            aria-label="模板编辑"
          >
            <div className="is-section-heading">
              <div>
                <h2>{editing.builtin ? '从内置模板创建副本' : '编辑模板'}</h2>
              </div>
              <IconButton label="关闭模板编辑" onClick={() => setEditing(null)}>
                <X size={18} />
              </IconButton>
            </div>
            <div className="is-two-cols">
              <Field label="模板名称">
                <input
                  className="kv-input"
                  value={editing.data.name}
                  onChange={(e) => editData({ name: e.target.value })}
                />
              </Field>
              <Field label="商品品类">
                <input
                  className="kv-input"
                  value={editing.data.category || ''}
                  onChange={(e) => editData({ category: e.target.value })}
                />
              </Field>
            </div>
            <Field label="图内语言">
              <ImageLanguageSelect
                value={editing.data.language || ''}
                onChange={(language) => editData({ language })}
              />
            </Field>
            <Field label="统一风格">
              <textarea
                className="kv-textarea custom-scrollbar"
                rows={4}
                value={editing.data.style || ''}
                onChange={(e) => editData({ style: e.target.value })}
                placeholder="色调、光线、字体、构图、留白，以及所有页面要共同遵守的要求"
              />
            </Field>
            <h3>
              页面安排 <span className="is-muted">{editing.data.slots.length} 页</span>
            </h3>
            {editing.data.slots.map((slot, index) => (
              <div className="is-slot-editor" key={`${index}`}>
                <span className="is-step-number">{index + 1}</span>
                <div>
                  <div className="is-two-cols">
                    <Field label="页面 ID">
                      <input
                        className="kv-input"
                        value={slot.id}
                        onChange={(e) =>
                          editData({
                            slots: editing.data.slots.map((s, i) =>
                              i === index ? { ...s, id: e.target.value } : s,
                            ),
                          })
                        }
                      />
                    </Field>
                    <Field label="页面用途">
                      <input
                        className="kv-input"
                        value={slot.purpose || ''}
                        onChange={(e) =>
                          editData({
                            slots: editing.data.slots.map((s, i) =>
                              i === index ? { ...s, purpose: e.target.value } : s,
                            ),
                          })
                        }
                      />
                    </Field>
                  </div>
                  {slot.example && <small className="is-muted">样图：{slot.example}</small>}
                  <Field label={editing.data.mode === 'replace' ? '换货规则' : '画面要求'}>
                    <textarea
                      className="kv-textarea custom-scrollbar"
                      rows={3}
                      value={(editing.data.mode === 'replace' ? slot.prompt : slot.brief) || ''}
                      onChange={(e) =>
                        editData({
                          slots: editing.data.slots.map((s, i) =>
                            i === index
                              ? {
                                  ...s,
                                  [editing.data.mode === 'replace' ? 'prompt' : 'brief']:
                                    e.target.value,
                                }
                              : s,
                          ),
                        })
                      }
                    />
                  </Field>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={editing.data.slots.length <= 1}
                    onClick={() =>
                      editData({
                        slots: editing.data.slots.filter((_, i) => i !== index),
                      })
                    }
                  >
                    移除此页
                  </Button>
                </div>
              </div>
            ))}
            {editing.data.mode === 'smart' && (
              <Button
                disabled={editing.data.slots.length >= 30}
                onClick={() =>
                  editData({
                    slots: [
                      ...editing.data.slots,
                      {
                        id: `page-${crypto.randomUUID().slice(0, 6)}`,
                        purpose: '',
                        brief: '',
                      },
                    ],
                  })
                }
              >
                <Plus size={15} />
                添加页面
              </Button>
            )}
            <div className="is-dialog-actions">
              <Button onClick={() => setEditing(null)}>取消</Button>
              <Button
                variant="primary"
                disabled={pending}
                onClick={() =>
                  void perform(async () => {
                    onChange(await api.imageStudioTemplateSave(editing))
                    setEditing(null)
                  })
                }
              >
                <Save size={15} />
                保存模板
              </Button>
            </div>
          </section>
        </div>
      )}
    </div>
  )
}
