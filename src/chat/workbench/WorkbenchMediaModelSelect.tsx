import { MediaGenerationRunner } from './MediaGenerationRunner'
import { useEffect, useState, type ReactNode } from 'react'
import { getSettingsCached, subscribeSettings } from '../../api/settingsCache'
import type { ModelProvider, Settings } from '../../api/tauri'
import { useLang } from '../../components/i18n'
import { mediaPoolEntries, type MediaPoolKind } from '../../data/mediaModelPools'
import { Select } from '../../settings/public/controls'
import { Button } from '../../components/Button'

const choiceKey = (kind: MediaPoolKind) => `dsivio.workbench.media-choice.${kind}`
function readChoice(kind: MediaPoolKind) {
  try { return localStorage.getItem(choiceKey(kind)) || '' } catch { return '' }
}

/** Workbench-local choice, never an assignment to the chat generation tool. */
export function WorkbenchMediaModelSelect({ kind, children, render }: {
  kind: MediaPoolKind
  children?: ReactNode
  render?: (control: ReactNode, provider: ModelProvider | undefined, model: string) => ReactNode
}) {
  const zh = useLang() === 'zh'
  const [settings, setSettings] = useState<Settings | null>(null)
  const [choice, setChoice] = useState(() => readChoice(kind))
  const [error, setError] = useState('')
  const [reload, setReload] = useState(0)
  useEffect(() => {
    let active = true, updated = false
    const unsubscribe = subscribeSettings(value => { updated = true; if (active) { setSettings(value); setError('') } })
    void getSettingsCached().then(value => { if (active && !updated) { setSettings(value); setError('') } }).catch(failure => { if (active && !updated) setError(String(failure)) })
    return () => { active = false; unsubscribe() }
  }, [reload])
  const entries = settings ? mediaPoolEntries(settings, kind).filter(entry => entry.available) : []
  const selected = entries.find(entry => entry.key === choice)
  const title = kind === 'imageModels' ? (zh ? '图片模型' : 'Image model') : (zh ? '视频模型' : 'Video model')
  const provider = settings?.providers.find(p => p.id === selected?.providerId)
  const control = <div className="workbench-field">
    <span>{title}</span>
    <Select ariaLabel={title} value={selected?.key || ''} disabled={!settings || !entries.length}
      options={[{value:'',label: settings ? (zh ? '请选择本次使用的模型' : 'Choose a model for this task') : (zh ? '正在加载模型池…' : 'Loading model pool…')}, ...entries.map(entry => ({value:entry.key,label:entry.label}))]}
      onChange={value => { setChoice(value); try { localStorage.setItem(choiceKey(kind), value) } catch { /* Selection still works for this mounted page. */ } }} />
    {settings && !entries.length && <p className="workbench-page-sub">{zh ? '请先在「设置 → 媒体创作」中添加可用模型。' : 'Add available models under Settings → Media creation first.'}</p>}
    {settings && entries.length > 0 && choice && !selected && <p className="workbench-page-sub">{zh ? '上次选择的模型已不可用，请重新选择。' : 'Your previous model is unavailable. Choose another model.'}</p>}
    {error && <div role="alert"><p className="workbench-page-sub">{zh ? '模型池加载失败' : 'Failed to load model pool'}: {error}</p><Button size="sm" onClick={() => { setError(''); setReload(value => value + 1) }}>{zh ? '重试' : 'Retry'}</Button></div>}
  </div>
  if (render) return render(control, provider, selected?.model || '')
  return <>{control}
    {children}
    {provider && selected && provider.request.comfy && <MediaGenerationRunner key={`${provider.id}:${selected.model}`} provider={provider} model={selected.model} kind={kind === 'imageModels' ? 'image' : 'video'} />}
  </>
}
