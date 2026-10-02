import { useMemo, useRef, useState, type ReactNode } from 'react'
import { Cpu, GripHorizontal } from 'lucide-react'
import { api, type DefaultModelSelection, type LocalAsrConfig, type Settings } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { ProviderIcon } from '../../components/ModelIcon'
import { isMediaPoolCandidate, mediaModelName, mediaModelKey, mediaPoolEntries, MEDIA_KIND_LABEL, type MediaPoolKind } from '../../data/mediaModelPools'
import { LOCAL_TRANSCRIBE_MODEL } from '../../data/speechModels'
import { Button } from '../../components/Button'
import { moveIdToIndex, usePointerReorder } from '../../utils/pointerReorder'
import { SettingsGroup } from '../components'
import { Input, Toggle } from '../public/controls'
import type { SettingsTab } from '../SettingsShell'
import { LocalAsrPanel } from './MediaLocalAsrPanel'
import { MediaVoicesPanel } from './MediaVoicesPanel'
import { MediaLocalSpeechPanel } from './MediaLocalSpeechPanel'

type MediaCreationProps = {
  settings: Settings
  lang: Lang
  onUpdatePool: (kind: MediaPoolKind, models: DefaultModelSelection[]) => void
  onUpdateLocalAsr?: (config: LocalAsrConfig) => void
  onNavigateTab?: (tab: SettingsTab) => void
}

/** Search only earns its space once the list no longer fits at a glance. */
const SEARCH_THRESHOLD = 6

type PoolRow = DefaultModelSelection & { key: string; name: string; available: boolean }

/** The media type is chosen in the page header (SettingsShell), like the usage page's views. */
export function MediaCreationTab(props: MediaCreationProps & { kind?: MediaPoolKind }) {
  const { settings, lang, kind = 'imageModels' } = props
  return <>
    {kind === 'speechModels' && <MediaLocalSpeechPanel lang={lang} />}
    <MediaModelPool key={kind} {...props} kind={kind} />
    {kind === 'speechModels' && <MediaVoicesPanel settings={settings} lang={lang} />}
    {kind === 'transcribeModels' && settings.workbenchMedia.localAsr
      && <LocalAsrPanel config={settings.workbenchMedia.localAsr} lang={lang} onUpdate={props.onUpdateLocalAsr} />}
  </>
}

function MediaModelPool({ settings, lang, onUpdatePool, onNavigateTab, kind }: MediaCreationProps & { kind: MediaPoolKind }) {
  const [query, setQuery] = useState('')
  const listRef = useRef<HTMLUListElement>(null)
  const zh = lang === 'zh'
  const label = MEDIA_KIND_LABEL[kind][zh ? 'zh' : 'en']
  const title = zh ? `${label}模型池` : `${label} model pool`
  const entries: PoolRow[] = mediaPoolEntries(settings, kind)
  const selected = new Set(entries.map(entry => entry.key))
  const candidates: PoolRow[] = [
    ...(kind === 'transcribeModels' ? [{ ...LOCAL_TRANSCRIBE_MODEL, name: 'WhisperX small', key: mediaModelKey(LOCAL_TRANSCRIBE_MODEL), available: true }] : []),
    ...settings.providers.flatMap(provider => provider.enabledModels
      .filter(model => isMediaPoolCandidate(provider, model, kind))
      .map(model => ({ providerId: provider.id, model, name: mediaModelName(provider, model), key: mediaModelKey({ providerId: provider.id, model }), available: true }))),
  ].filter(entry => !selected.has(entry.key))
  const providerName = (providerId: string) => providerId === 'local'
    ? (zh ? '本地' : 'Local')
    : settings.providers.find(provider => provider.id === providerId)?.name || providerId
  const search = query.trim().toLocaleLowerCase()
  const matches = (entry: PoolRow) => `${providerName(entry.providerId)} ${entry.name} ${entry.model}`.toLocaleLowerCase().includes(search)
  const shownEntries = entries.filter(matches)
  const shownCandidates = candidates.filter(matches)
  const showSearch = entries.length + candidates.length > SEARCH_THRESHOLD || search !== ''
  // Dragging works on the whole pool, so it is offered only when no search hides part of it.
  const reorderable = search === '' && entries.length > 1
  const ids = useMemo(() => entries.map(entry => entry.key), [entries])
  const { draggingId, startDrag, itemTransform } = usePointerReorder({
    ids,
    listRef,
    itemSelector: '.kv-media-row',
    rowGap: 0,
    onReorder: (fromId, toId) => {
      const order = moveIdToIndex(ids, fromId, ids.indexOf(toId))
      const byKey = new Map(entries.map(entry => [entry.key, entry]))
      onUpdatePool(kind, order.map(key => byKey.get(key)!).map(({ providerId, model }) => ({ providerId, model })))
    },
  })
  const toggle = (entry: PoolRow, checked: boolean) => {
    const models = entries.filter(item => item.key !== entry.key).map(({ providerId, model }) => ({ providerId, model }))
    // A newly enabled model joins at the end so it never silently replaces the default.
    onUpdatePool(kind, checked ? [...models, { providerId: entry.providerId, model: entry.model }] : models)
  }
  const goToModels = onNavigateTab && <Button size="sm" onClick={() => onNavigateTab('providers')}>{zh ? '前往模型设置' : 'Open model settings'}</Button>
  const kindHint = {
    imageModels: null,
    videoModels: null,
    speechModels: zh
      ? '在「模型」里添加官方的语音模型（OpenAI、MiniMax）后会自动出现在这里。供应商 Key 不代表已开通语音产品，MiniMax 需要按量付费的语音 Key。'
      : 'Speech models from OpenAI or MiniMax appear here once you add them under Models. A provider key does not imply speech access; MiniMax needs a pay-as-you-go speech key.',
    transcribeModels: zh
      ? '本地 WhisperX 无需 API Key。云端转写（OpenAI whisper-1）在「模型」里添加后出现在这里，会上传音频并可能收费，只在你选择它时使用；本地失败不会自动改用云端。'
      : 'Local WhisperX needs no API key. Cloud transcription uploads audio and may charge; it runs only when chosen and is never a fallback for local failures.',
  }[kind]

  const row = (entry: PoolRow, index: number) => {
    const isSelected = index >= 0
    const isDefault = index === 0 && entry.available
    const transform = isSelected ? itemTransform(index) : undefined
    const name = providerName(entry.providerId)
    const provider = settings.providers.find(item => item.id === entry.providerId)
    return <li key={entry.key} className={`kv-media-row${draggingId === entry.key ? ' is-dragging' : ''}`}
      style={transform ? { transform } : undefined} data-tauri-drag-region="false">
      {isSelected && (reorderable
        ? <button type="button" className="kv-provider-drag-handle" aria-label={`${zh ? '拖动调整优先级' : 'Drag to change priority'}: ${name} / ${entry.name}`}
          title={zh ? '拖动调整优先级' : 'Drag to change priority'}
          onPointerDown={event => startDrag(event, entry.key, index)} onClick={event => event.stopPropagation()} data-tauri-drag-region="false">
          <GripHorizontal size={13} strokeWidth={2} />
        </button>
        : entries.length > 1 && <span className="kv-media-handle-space" aria-hidden="true" />)}
      {entry.providerId === 'local'
        ? <span className="kv-media-local-icon" aria-hidden="true"><Cpu size={13} /></span>
        : <ProviderIcon name={provider?.name || entry.providerId} baseUrl={provider?.baseUrl} iconKey={settings.providerIcons?.[entry.providerId]} size={16} />}
      <div className="min-w-0 flex-1">
        <div className="kv-media-row-name">
          <span className="kv-row-label [overflow-wrap:anywhere]">{entry.name}</span>
          {isDefault && <span className="kv-tag accent">{zh ? '默认' : 'Default'}</span>}
          {!entry.available && <span className="kv-tag warn">{zh ? '不可用' : 'Unavailable'}</span>}
        </div>
        <p className="kv-row-desc">{entry.available ? name : `${name} · ${zh ? '当前不可用：供应商已停用或模型已移除' : 'Unavailable: the provider is off or the model was removed'}`}</p>
        {isSelected && kind === 'speechModels' && entry.available && <SpeechCheck providerId={entry.providerId} model={entry.model} lang={lang} />}
      </div>
      <Toggle ariaLabel={`${name} / ${entry.name}`} checked={isSelected} onChange={checked => toggle(entry, checked)} />
    </li>
  }

  return <section role="region" aria-label={title}>
    {kindHint && <div className="kv-media-hint">
      <p className="kv-row-desc">{kindHint}</p>
      {kind === 'speechModels' && candidates.length > 0 && goToModels}
    </div>}
    {showSearch && <div className="kv-media-search">
      <Input type="search" aria-label={zh ? `搜索${label}模型` : `Search ${label.toLowerCase()} models`} value={query} onChange={setQuery}
        placeholder={zh ? '搜索供应商或模型' : 'Search providers or models'} />
    </div>}

    <SettingsGroup title={<GroupTitle text={kind === 'speechModels' ? (zh ? '已启用的云端语音' : 'Enabled cloud speech') : (zh ? '已启用' : 'Enabled')} count={entries.length}
      hint={entries.length > 1 ? (zh ? '第一个是默认，拖动 ⋮⋮ 调整顺序' : 'The first is the default. Drag to reorder.') : undefined} />}>
      {entries.length === 0
        ? <p className="kv-row-desc kv-media-empty">{candidates.length > 0
          ? (zh ? `还没有启用${label}模型。在下方打开一个即可使用。` : `No ${label.toLowerCase()} model is enabled yet. Turn one on below.`)
          : (zh ? `还没有可用的${kind === 'speechModels' ? '云端语音' : label}模型。` : `No ${label.toLowerCase()} models are available yet.`)}</p>
        : <ul ref={listRef} className={`kv-media-list${draggingId ? ' is-sorting' : ''}`}>
          {entries.map((entry, index) => shownEntries.includes(entry) ? row(entry, index) : null)}
        </ul>}
    </SettingsGroup>

    {(candidates.length > 0 || entries.length === 0) && <SettingsGroup title={<GroupTitle text={kind === 'speechModels' ? (zh ? '可添加的云端语音' : 'Available cloud speech') : (zh ? '可添加' : 'Available')} count={candidates.length} />}>
      {candidates.length === 0
        ? <div className="kv-media-empty">
          <p className="kv-row-desc">{zh
            ? `「模型」里还没有能生成${label}的已启用模型。先添加供应商并启用对应模型。`
            : `No enabled model under Models can produce ${label.toLowerCase()}. Add a provider and enable a matching model first.`}</p>
          {goToModels}
        </div>
        : <ul className="kv-media-list">{shownCandidates.map(entry => row(entry, -1))}</ul>}
    </SettingsGroup>}

    {search !== '' && shownEntries.length === 0 && shownCandidates.length === 0
      && <p className="kv-row-desc kv-media-empty" role="status">{zh ? '没有匹配的模型' : 'No matching models'}</p>}
  </section>
}

function GroupTitle({ text, count, hint }: { text: string; count: number; hint?: ReactNode }) {
  return <div className="kv-media-group-title">
    <span>{text} <span className="kv-media-count">{count}</span></span>
    {hint && <span className="kv-media-group-hint">{hint}</span>}
  </div>
}

/** Checks the speech product configuration for one pool member; the result stays on its row. */
function SpeechCheck({ providerId, model, lang }: { providerId: string; model: string; lang: Lang }) {
  const zh = lang === 'zh'
  const [state, setState] = useState<{ busy: boolean; ok?: boolean; message?: string }>({ busy: false })
  async function check() {
    setState({ busy: true })
    try {
      const result = await api.checkMediaSpeechConnection(providerId, model)
      setState({ busy: false, ok: result.configured && result.authenticated, message: result.message })
    } catch (failure) {
      setState({ busy: false, ok: false, message: String(failure) })
    }
  }
  return <div className="kv-media-check">
    <Button size="sm" variant="ghost" disabled={state.busy} onClick={() => void check()}>{state.busy ? (zh ? '检查中…' : 'Checking…') : (zh ? '检查配置' : 'Check configuration')}</Button>
    {state.message && <span className={`kv-row-desc [overflow-wrap:anywhere] ${state.ok ? '' : 'kv-media-error-text'}`} role={state.ok ? 'status' : 'alert'}>{state.message}</span>}
  </div>
}
