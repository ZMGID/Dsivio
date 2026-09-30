import { useId, useMemo, useRef, useState } from 'react'
import { GripHorizontal } from 'lucide-react'
import type { DefaultModelSelection, Settings } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { isMediaPoolCandidate, mediaModelName, mediaModelKey, mediaPoolEntries, type MediaPoolKind } from '../../data/mediaModelPools'
import { moveIdToIndex, usePointerReorder } from '../../utils/pointerReorder'
import { SettingsGroup } from '../components'
import { Input, Toggle } from '../public/controls'

type MediaCreationProps = {
  settings: Settings
  lang: Lang
  onUpdatePool: (kind: MediaPoolKind, models: DefaultModelSelection[]) => void
}

function MediaModelPool({ settings, lang, onUpdatePool, kind }: MediaCreationProps & { kind: MediaPoolKind }) {
  const [query, setQuery] = useState('')
  const searchId = useId()
  const listRef = useRef<HTMLUListElement>(null)
  const zh = lang === 'zh'
  const isImage = kind === 'imageModels'
  const title = isImage ? (zh ? '图片模型池' : 'Image model pool') : (zh ? '视频模型池' : 'Video model pool')
  const searchLabel = isImage ? (zh ? '搜索图片模型' : 'Search image models') : (zh ? '搜索视频模型' : 'Search video models')
  const dragLabel = zh ? '拖动调整优先级' : 'Drag to change priority'
  const entries = mediaPoolEntries(settings, kind)
  const selected = new Set(entries.map(entry => entry.key))
  const candidates = settings.providers.flatMap(provider => provider.enabledModels
    .filter(model => isMediaPoolCandidate(provider, model, kind))
    .map(model => ({ providerId: provider.id, model, name: mediaModelName(provider, model), key: mediaModelKey({ providerId: provider.id, model }), available: true })))
  // Selected models come first in pool order, because that order is the priority (the first one is
  // the default for Workbench, chat tools and `dsivio media`). Unselected candidates follow.
  // Unavailable members stay visible so they can still be removed from the pool.
  const rows = [...entries, ...candidates.filter(entry => !selected.has(entry.key))]
  const search = query.trim().toLocaleLowerCase()
  const filtered = rows.filter(entry => {
    const providerName = settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
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
          const providerName = settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
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
    </SettingsGroup>
  </div>
}

export function MediaCreationTab(props: MediaCreationProps) {
  return <>{(['imageModels', 'videoModels'] as const).map(kind => <MediaModelPool key={kind} kind={kind} {...props} />)}</>
}
