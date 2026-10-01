import { useEffect, useId, useMemo, useRef, useState } from 'react'
import { GripHorizontal } from 'lucide-react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, type DefaultModelSelection, type LocalAsrConfig, type LocalAsrStatus, type Settings, type VoiceReference } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { isMediaPoolCandidate, mediaModelName, mediaModelKey, mediaPoolEntries, type MediaPoolKind } from '../../data/mediaModelPools'
import { LOCAL_TRANSCRIBE_MODEL } from '../../data/speechModels'
import { Button } from '../../components/Button'
import { moveIdToIndex, usePointerReorder } from '../../utils/pointerReorder'
import { SettingsGroup } from '../components'
import { FieldBlock, Input, Toggle } from '../public/controls'

type MediaCreationProps = {
  settings: Settings
  lang: Lang
  onUpdatePool: (kind: MediaPoolKind, models: DefaultModelSelection[]) => void
  onUpdateLocalAsr?: (config: LocalAsrConfig) => void
}

function MediaModelPool({ settings, lang, onUpdatePool, kind }: MediaCreationProps & { kind: MediaPoolKind }) {
  const [query, setQuery] = useState('')
  const searchId = useId()
  const listRef = useRef<HTMLUListElement>(null)
  const zh = lang === 'zh'
  const title = {
    imageModels: zh ? '图片模型池' : 'Image model pool',
    videoModels: zh ? '视频模型池' : 'Video model pool',
    speechModels: zh ? '语音模型池' : 'Speech model pool',
    transcribeModels: zh ? '转写模型池' : 'Transcription model pool',
  }[kind]
  const searchLabel = {
    imageModels: zh ? '搜索图片模型' : 'Search image models',
    videoModels: zh ? '搜索视频模型' : 'Search video models',
    speechModels: zh ? '搜索语音模型' : 'Search speech models',
    transcribeModels: zh ? '搜索转写模型' : 'Search transcription models',
  }[kind]
  const dragLabel = zh ? '拖动调整优先级' : 'Drag to change priority'
  const entries = mediaPoolEntries(settings, kind)
  const selected = new Set(entries.map(entry => entry.key))
  const candidates = [
    ...(kind === 'transcribeModels' ? [{ ...LOCAL_TRANSCRIBE_MODEL, name: 'WhisperX small', key: mediaModelKey(LOCAL_TRANSCRIBE_MODEL), available: true }] : []),
    ...settings.providers.flatMap(provider => provider.enabledModels
      .filter(model => isMediaPoolCandidate(provider, model, kind))
      .map(model => ({ providerId: provider.id, model, name: mediaModelName(provider, model), key: mediaModelKey({ providerId: provider.id, model }), available: true }))),
  ]
  // Selected models come first in pool order, because that order is the priority (the first one is
  // the default for Workbench, chat tools and `dsivio media`). Unselected candidates follow.
  // Unavailable members stay visible so they can still be removed from the pool.
  const rows = [...entries, ...candidates.filter(entry => !selected.has(entry.key))]
  const search = query.trim().toLocaleLowerCase()
  const filtered = rows.filter(entry => {
    const providerName = entry.providerId === 'local' ? (zh ? '本地' : 'Local') : settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
    return `${providerName} ${entry.name} ${entry.model}`.toLocaleLowerCase().includes(search)
  })
  // Dragging works on the whole pool, so it is offered only when no search hides part of it.
  const reorderable = search === '' && entries.length > 1
  const ids = useMemo(() => entries.map(entry => entry.key), [entries])
  const { draggingId, startDrag, itemTransform } = usePointerReorder({
    ids,
    listRef,
    itemSelector: '.kv-media-model-row',
    onReorder: (fromId, toId) => {
      const order = moveIdToIndex(ids, fromId, ids.indexOf(toId))
      const byKey = new Map(entries.map(entry => [entry.key, entry]))
      onUpdatePool(kind, order.map(key => byKey.get(key)!).map(({ providerId, model }) => ({ providerId, model })))
    },
  })

  return <div role="region" aria-label={title}>
    <SettingsGroup title={<div className="flex min-w-0 items-baseline justify-between gap-4">
      <h2 className="text-sm font-semibold text-foreground">{title}</h2>
      <span className="shrink-0">{zh ? `已选 ${entries.length} 个` : `${entries.length} selected`}</span>
    </div>}>
      {rows.length > 0 ? <>
        <div className="flex min-w-0 flex-col gap-2 py-3">
          <label htmlFor={searchId} className="kv-row-desc">{searchLabel}</label>
          <Input id={searchId} type="search" value={query} onChange={setQuery} placeholder={zh ? '输入供应商或模型名称' : 'Provider or model name'} />
          {entries.length > 1 && <p className="kv-row-desc">{zh ? '已选模型按顺序排列，靠前的优先，第一个是默认模型。拖动 ⋮⋮ 调整。' : 'Selected models are ordered by priority. The first is the default. Drag the handle to reorder.'}</p>}
        </div>
        {filtered.length === 0 && <p className="kv-row-desc py-3" role="status">{zh ? '没有匹配的模型' : 'No matching models'}</p>}
        <ul ref={listRef} className={`divide-y divide-border${draggingId ? ' kv-provider-list-items is-sorting' : ''}`}>{filtered.map(entry => {
          const providerName = entry.providerId === 'local' ? (zh ? '本地' : 'Local') : settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
          const index = ids.indexOf(entry.key)
          const isSelected = index >= 0
          const transform = isSelected ? itemTransform(index) : undefined
          const isDefault = isSelected && index === 0
          return <li key={entry.key} className={`kv-media-model-row kv-provider-item flex min-w-0 items-center gap-4 py-3${draggingId === entry.key ? ' is-dragging' : ''}`}
            style={transform ? { transform } : undefined} data-tauri-drag-region="false">
            {reorderable && isSelected
              ? <button type="button" className="kv-provider-drag-handle" aria-label={`${dragLabel}: ${providerName} / ${entry.name}`} title={dragLabel}
                onPointerDown={event => startDrag(event, entry.key, index)} onClick={event => event.stopPropagation()} data-tauri-drag-region="false">
                <GripHorizontal size={13} strokeWidth={2} />
              </button>
              : <span className="kv-provider-drag-handle" aria-hidden="true" style={{ visibility: 'hidden' }} />}
            <div className="min-w-0 flex-1 [overflow-wrap:anywhere]">
              <div className="kv-row-label">{entry.name}{isDefault && <span className="kv-row-desc ml-2">{zh ? '默认' : 'Default'}</span>}</div>
              <p className="kv-row-desc">{entry.available ? providerName : `${providerName} · ${zh ? '当前不可用，请检查供应商和模型配置' : 'Unavailable. Check provider and model settings.'}`}</p>
            </div>
            <Toggle ariaLabel={`${providerName} / ${entry.name}`} checked={selected.has(entry.key)} onChange={checked => {
              const models = entries.filter(item => item.key !== entry.key).map(({ providerId, model }) => ({ providerId, model }))
              onUpdatePool(kind, checked ? [...models, { providerId: entry.providerId, model: entry.model }] : models)
            }} />
          </li>
        })}</ul>
      </> : <p className="kv-row-desc py-3">{zh ? '暂无可添加的模型。请先在「模型」中配置供应商并启用对应的生成模型。' : 'No models available. Configure a provider and enable its generation models under Models first.'}</p>}
      {kind === 'speechModels' && <p className="kv-row-desc py-3">{zh ? '在模型详情明确填写语音协议和产品地址后加入。使用供应商 Key 不代表已开通语音产品；MiniMax Token Plan、Doubao Ark 不等于语音权限。' : 'Set the speech protocol and product URL in Model details first. A provider key does not imply speech access; MiniMax Token Plan and Doubao Ark are separate products.'}</p>}
      {kind === 'transcribeModels' && <p className="kv-row-desc py-3">{zh ? '本地 WhisperX 不需要 API Key。云转写会上传音频并可能产生费用，仅按你明确选择的模型执行；本地失败不会自动转云。' : 'Local WhisperX needs no API key. Cloud transcription uploads audio and may incur charges; it runs only when explicitly chosen, never as a local failure fallback.'}</p>}
    </SettingsGroup>
  </div>
}

export function MediaCreationTab(props: MediaCreationProps) {
  return <>
    {(['imageModels', 'videoModels', 'speechModels', 'transcribeModels'] as const).map(kind => <MediaModelPool key={kind} kind={kind} {...props} />)}
    <LocalAsrSettings {...props} />
    <MediaVoices settings={props.settings} lang={props.lang} />
  </>
}

function LocalAsrSettings({ settings, lang, onUpdateLocalAsr }: MediaCreationProps) {
  const zh = lang === 'zh'
  const config = settings.workbenchMedia.localAsr
  const [status, setStatus] = useState<LocalAsrStatus | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [reload, setReload] = useState(0)
  const mounted = useRef(false)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout>
    async function refresh() {
      try { const next = await api.getLocalAsrStatus(); if (active) setStatus(next) }
      catch (failure) { if (active) setError(String(failure)) }
      finally { if (active) timer = setTimeout(() => void refresh(), 2500) }
    }
    void refresh()
    return () => { active = false; clearTimeout(timer) }
  }, [reload])
  const stateLabels: Record<string, string> = zh
    ? { notInstalled: '未安装', installing: '安装中', ready: '已安装', failed: '安装失败', cancelled: '安装已取消' }
    : { notInstalled: 'Not installed', installing: 'Installing', ready: 'Installed', failed: 'Installation failed', cancelled: 'Installation cancelled' }
  const runtimeLabels: Record<string, string> = zh
    ? { stopped: '已停止', starting: '启动中', ready: '就绪', busy: '正在转写', stopping: '停止中' }
    : { stopped: 'Stopped', starting: 'Starting', ready: 'Ready', busy: 'Transcribing', stopping: 'Stopping' }
  async function act(action: 'install' | 'stop' | 'cancel') {
    if (busy || !config) return
    setBusy(true); setError(''); setNotice('')
    try {
      if (action === 'install') await api.installLocalAsr(config)
      else if (action === 'cancel' && status?.operationId) await api.cancelLocalAsrInstall(status.operationId)
      else if (action === 'stop') {
        const result = await api.stopLocalAsr()
        if (mounted.current) setNotice(`${zh ? '停止结果' : 'Stop outcome'}: ${result.outcome}`)
      }
      if (mounted.current) setReload(value => value + 1)
    } catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { if (mounted.current) setBusy(false) }
  }
  return <SettingsGroup title={zh ? '本地逐字转写 · WhisperX' : 'Local word transcription · WhisperX'}>
    <p className="kv-row-desc py-3">{zh ? 'App 安装与监督本地服务；安装时下载依赖和模型，推理离线。首次实际转写可自动安装，规划和启动 App 不触发安装。安装失败不替换旧可用环境，不自动上传到云端。' : 'The App installs and supervises the service. Installation downloads dependencies and models; inference is offline. Only an actual transcription triggers automatic installation, not planning or App startup. Failure keeps the previous working installation and never uploads to the cloud.'}</p>
    {config && <>
      <FieldBlock label={zh ? '期望模型' : 'Desired model'}><Input aria-label={zh ? '期望模型' : 'Desired model'} value={config.model} onChange={() => {}} disabled /></FieldBlock>
      <FieldBlock label={zh ? '安装语言（两至三位代码，逗号分隔）' : 'Installed languages (2–3 letter codes, comma separated)'}>
        <Input aria-label={zh ? '安装语言' : 'Installed languages'} value={config.languages.join(', ')} disabled={busy || status?.state === 'installing' || !onUpdateLocalAsr}
          onChange={value => onUpdateLocalAsr?.({ ...config, languages: value.split(',').map(language => language.trim()) })} />
      </FieldBlock>
      <div className="flex items-center justify-between gap-4 py-3"><span className="kv-row-label">{zh ? '首次需要时自动安装' : 'Automatically install when first needed'}</span>
        <Toggle ariaLabel={zh ? '首次需要时自动安装' : 'Automatically install when first needed'} checked={config.autoInstall} disabled={!onUpdateLocalAsr}
          onChange={autoInstall => onUpdateLocalAsr?.({ ...config, autoInstall })} /></div>
    </>}
    <div role="status" className="kv-row-desc py-3">
      {status ? <>
        <p>{stateLabels[status.state] || status.state} · {status.serviceVersion} · {status.model} · {status.languages.join(', ')}</p>
        <p>{zh ? '运行状态' : 'Runtime'}: {runtimeLabels[status.runtime.state] || status.runtime.state}{status.runtime.activeTaskId ? ` · ${status.runtime.activeTaskId}` : ''}</p>
        {status.progress && <p>{status.progress.stage} · {status.progress.message}</p>}
      </> : (zh ? '正在读取安装状态…' : 'Loading installation status…')}
      {notice && <p>{notice}</p>}
    </div>
    {(error || status?.error) && <p className="kv-row-desc py-3 [overflow-wrap:anywhere]" role="alert">{error || status?.error}</p>}
    <div className="flex flex-wrap gap-2 py-3">
      <Button size="sm" disabled={busy || !config || status?.state === 'installing'} onClick={() => void act('install')}>{status?.state === 'failed' ? (zh ? '重试安装' : 'Retry installation') : (zh ? '手动安装／更新语言' : 'Install / update languages')}</Button>
      {status?.state === 'installing' && <Button size="sm" disabled={busy || !status.operationId} onClick={() => void act('cancel')}>{zh ? '取消本次安装' : 'Cancel this installation'}</Button>}
      <Button size="sm" disabled={busy || !status || status.runtime.state === 'stopped' || status.runtime.state === 'busy'} onClick={() => void act('stop')}>{zh ? '停止空闲服务' : 'Stop idle service'}</Button>
      <Button size="sm" disabled={busy} onClick={() => { setError(''); setReload(value => value + 1) }}>{zh ? '刷新状态' : 'Refresh status'}</Button>
    </div>
    {status?.runtime.state === 'busy' && <p className="kv-row-desc py-3">{zh ? '服务正在处理任务；请在媒体任务列表显式取消该任务。停止空闲服务不会中断其他任务。' : 'A task is active. Cancel that task explicitly in the media task list. Stopping an idle service never interrupts other tasks.'}</p>}
  </SettingsGroup>
}

function MediaVoices({ settings, lang }: Pick<MediaCreationProps, 'settings' | 'lang'>) {
  const zh = lang === 'zh'
  const [voices, setVoices] = useState<VoiceReference[]>([])
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [busy, setBusy] = useState(false)
  const [reload, setReload] = useState(0)
  const mounted = useRef(false)
  const voiceRevision = useRef(0)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  useEffect(() => {
    let active = true
    const revision = ++voiceRevision.current
    void api.listMediaVoices().then(result => { if (active && voiceRevision.current === revision) setVoices(result) })
      .catch(failure => { if (active && voiceRevision.current === revision) setError(String(failure)) })
    return () => { active = false }
  }, [reload])
  async function remove(id: string) {
    voiceRevision.current += 1
    setBusy(true); setError(''); setNotice('')
    try {
      await api.deleteMediaVoice(id)
      if (mounted.current) { setVoices(current => current.filter(voice => voice.id !== id)); setNotice(zh ? '已删除本地引用；未删除供应商声音或任务产物。' : 'Local reference deleted. Provider voice and task artifacts were not deleted.') }
    } catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { if (mounted.current) setBusy(false) }
  }
  async function check(providerId: string, model: string) {
    setBusy(true); setError(''); setNotice('')
    try { const result = await api.checkMediaSpeechConnection(providerId, model); if (mounted.current) setNotice(result.message) }
    catch (failure) { if (mounted.current) setError(String(failure)) }
    finally { if (mounted.current) setBusy(false) }
  }
  return <SettingsGroup title={zh ? '声音引用与试听' : 'Voice references and playback'}>
    <p className="kv-row-desc py-3">{zh ? '克隆必须由用户提供合法样本与真实授权声明，不得冒充他人或伪造同意。声明不等于 OpenAI 要求的本人同意录音；本阶段不提供 OpenAI 样本克隆。列表是本地引用，不保证供应商永久保存声音。试听播放已保存的音频产物，不重新付费合成。' : 'Cloning requires a lawful sample and a real user-provided consent attestation. Never impersonate or manufacture consent. An attestation is not OpenAI’s required consent recording; OpenAI sample cloning is not offered. These are local references, not a promise of permanent provider storage. Playback uses saved audio, not a new paid synthesis.'}</p>
    <div className="flex flex-wrap gap-2 py-3">
      <Button size="sm" disabled={busy} onClick={() => { setError(''); setReload(value => value + 1) }}>{zh ? '刷新声音列表' : 'Refresh voices'}</Button>
      {mediaPoolEntries(settings, 'speechModels').filter(entry => entry.available).map(entry => <Button key={entry.key} size="sm" disabled={busy} onClick={() => void check(entry.providerId, entry.model)}>{zh ? '检查配置' : 'Check configuration'} · {entry.label}</Button>)}
    </div>
    {notice && <p className="kv-row-desc py-3" role="status">{notice}</p>}
    {error && <p className="kv-row-desc py-3 [overflow-wrap:anywhere]" role="alert">{error}</p>}
    {!voices.length && <p className="kv-row-desc py-3">{zh ? '暂无本地声音引用。' : 'No local voice references.'}</p>}
    <ul className="divide-y divide-border">{voices.map(voice => <li key={voice.id} className="flex min-w-0 flex-col gap-2 py-3">
      <span className="kv-row-label [overflow-wrap:anywhere]">{voice.voiceId}</span>
      <p className="kv-row-desc">{settings.providers.find(provider => provider.id === voice.providerId)?.name || voice.providerId} · {voice.protocol} · {voice.used ? (zh ? '已使用' : 'Used') : (zh ? '尚未使用' : 'Unused')}</p>
      {voice.expiresAt && <p className="kv-row-desc">{zh ? '供应商临时声音有效期' : 'Temporary provider voice expires'}: {new Date(voice.expiresAt).toLocaleString()}</p>}
      {voice.preview && <audio controls preload="metadata" src={convertFileSrc(voice.preview.path)} aria-label={`${zh ? '试听' : 'Play'} ${voice.voiceId}`} />}
      <Button size="sm" className="self-start" disabled={busy} onClick={() => void remove(voice.id)}>{zh ? '删除本地引用' : 'Delete local reference'}</Button>
    </li>)}</ul>
  </SettingsGroup>
}
