import { WorkbenchMediaModelSelect } from '../../WorkbenchMediaModelSelect'
import { mediaModelKey } from '../../../../data/mediaModelPools'
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
import { Check, FolderOpen, Image as ImageIcon, Settings2, X } from 'lucide-react'
import { api, isTauriRuntime, type ModelProvider } from '../../../../api/tauri'
import { getSettingsCached } from '../../../../api/settingsCache'
import { Button, IconButton } from '../../../../components/Button'
import { Select } from '../../../../settings/public/controls'
import { ModelPairSelect } from '../../../../settings/public/modelSelection'
import type { ImageConfig } from './types'
import { builtinImageUrl } from './builtinTemplates'
import { inferImageStudioProtocol, isVisionModel, reconcileImageStudioProtocol } from './studioModels'

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
    if (!isValidElement<{ value?: string | number; children?: ReactNode }>(child)) return []
    const label = text(child.props.children)
    return [{ value: String(child.props.value ?? label), label }]
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
  allowFollowExample = false,
  allowAuto = false,
  inheritedValue,
}: {
  value: string
  onChange: (value: string) => void
  disabled?: boolean
  allowFollowExample?: boolean
  allowAuto?: boolean
  inheritedValue?: string
}) {
  const languages = allowFollowExample || value === '跟随样图'
    ? [['跟随样图', '跟随样图'], ...imageLanguages] : [...imageLanguages]
  if (allowAuto) languages.unshift(['auto', 'Auto'])
  if (inheritedValue && !languages.some(([code]) => code === inheritedValue)) {
    const label = imageLanguages.find(([code]) => inheritedValue.includes(code))?.[1]
    languages.unshift([inheritedValue, label ? `模板语言 · ${label}` : '跟随模板语言'])
  }
  const known = languages.some(([code]) => code === value)
  return (
    <div className="is-field">
      <StudioSelect
        ariaLabel="图内语言"
        disabled={disabled}
        value={known ? value : 'custom'}
        onChange={(e) => onChange(e.target.value === 'custom' ? '' : e.target.value)}
      >
        {languages.map(([code, label]) => (
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
        .workbenchImagePreview(path, large)
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
  const configVersion = useRef(config)
  const [configConflict, setConfigConflict] = useState(false)
  useEffect(() => {
    if (JSON.stringify(config) === JSON.stringify(configVersion.current)) return
    if (JSON.stringify(draft) !== JSON.stringify(configVersion.current)) {
      setConfigConflict(true)
      return
    }
    configVersion.current = config
    setDraft(config)
  }, [config, draft])
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
        aria-label="模型与保存位置"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="is-section-heading">
          <div>
            <Settings2 size={18} />
            <h2>模型与保存位置</h2>
          </div>
          <IconButton label="关闭设置" onClick={onClose}>
            <X size={18} />
          </IconButton>
        </div>
        {configConflict && <p role="status">配置已在聊天中更新，请先载入最新配置。<Button onClick={() => { configVersion.current = config; setDraft(config); setConfigConflict(false) }}>载入最新配置</Button></p>}
        <p className="is-muted">
          本次图片功能使用工作台模型池；供应商连接沿用全局设置。
        </p>
        <Field label="图片保存位置">
          <div className="is-actions">
            <input className="kv-input" readOnly value={draft.outputRoot || '系统图片目录 / dsivio / Images'} />
            <IconButton label="选择图片保存位置" disabled={pending || configConflict} onClick={() => {
              void open({ directory: true, title: '选择新任务的图片保存位置' }).then((path) => {
                if (typeof path === 'string') setDraft((d) => ({ ...d, outputRoot: path }))
              }).catch((e) => setError(String(e)))
            }}><FolderOpen size={16} /></IconButton>
          </div>
          <small>按任务分文件夹，原图自动保存。更换位置只影响新任务。</small>
        </Field>
        <div className="is-field">
          <span>图片模型</span>
          <WorkbenchMediaModelSelect kind="imageModels" value={mediaModelKey(draft)} onChange={(providerId, model) => setDraft(d => ({ ...d, providerId, model, protocol: inferImageStudioProtocol(providers.find(p => p.id === providerId), model) }))} render={control => control} />
          <small>模型来自设置中的媒体创作图片模型池。</small>
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
            disabled={pending || configConflict}
            onClick={() => {
              setPending(true)
              const next = reconcileImageStudioProtocol(
                draft,
                providers.find((p) => p.id === draft.providerId),
              )
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
