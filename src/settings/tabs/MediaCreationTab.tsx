import { useId, useState } from 'react'
import type { DefaultModelSelection, Settings } from '../../api/tauri'
import type { Lang } from '../../components/i18n'
import { isMediaPoolCandidate, mediaModelName, mediaModelKey, mediaPoolEntries, type MediaPoolKind } from '../../data/mediaModelPools'
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
  const zh = lang === 'zh'
  const isImage = kind === 'imageModels'
  const title = isImage ? (zh ? '图片模型池' : 'Image model pool') : (zh ? '视频模型池' : 'Video model pool')
  const searchLabel = isImage ? (zh ? '搜索图片模型' : 'Search image models') : (zh ? '搜索视频模型' : 'Search video models')
  const entries = mediaPoolEntries(settings, kind)
  const selected = new Set(entries.map(entry => entry.key))
  const candidates = settings.providers.flatMap(provider => provider.enabledModels
    .filter(model => isMediaPoolCandidate(provider, model, kind))
    .map(model => ({ providerId: provider.id, model, name: mediaModelName(provider, model), key: mediaModelKey({ providerId: provider.id, model }), available: true })))
  // Keep unavailable members visible so they can still be removed from the pool.
  const rows = [...candidates, ...entries.filter(entry => !entry.available)]
  const search = query.trim().toLocaleLowerCase()
  const filtered = rows.filter(entry => {
    const providerName = settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
    return `${providerName} ${entry.name} ${entry.model}`.toLocaleLowerCase().includes(search)
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
        </div>
        {filtered.length === 0 && <p className="kv-row-desc py-3" role="status">{zh ? '没有匹配的模型' : 'No matching models'}</p>}
        <ul className="divide-y divide-border">{filtered.map(entry => {
          const providerName = settings.providers.find(provider => provider.id === entry.providerId)?.name || entry.providerId
          return <li key={entry.key} className="flex min-w-0 items-center gap-4 py-3">
            <div className="min-w-0 flex-1 [overflow-wrap:anywhere]">
              <div className="kv-row-label">{entry.name}</div>
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
