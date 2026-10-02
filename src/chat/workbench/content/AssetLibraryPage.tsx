import { useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { FolderOpen } from 'lucide-react'
import { api } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { workbenchOrigin, useMediaGeneration } from '../useMediaGeneration'
import { AssetCards } from './AssetPicker'
import { ASSET_TABS } from './contentCatalog'
import { assetOrigins, filterAssets, type AssetQuery } from './assetLibrary'
import { useFileDrop } from '../useFileDrop'
import './assetLibrary.css'

const IMPORT_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'mp4', 'webm', 'mov', 'mp3', 'wav', 'm4a', 'srt', 'md', 'txt', 'markdown']

/**
 * 图片视频库：已完成的媒体任务是唯一列表。删除、打开、定位和导入都走现成命令。
 */
export function AssetLibraryPage() {
  const t = useT()
  const generation = useMediaGeneration({})
  const [query, setQuery] = useState<AssetQuery>({ kind: 'all', origin: '', keyword: '' })
  const [importing, setImporting] = useState(false)
  const completed = filterAssets(generation.tasks, { ...query, kind: 'all' })
  const shown = query.kind === 'all' ? completed : completed.filter((task) => task.kind === query.kind)
  const origins = assetOrigins(generation.tasks)

  async function importPaths(paths: string[]) {
    setImporting(true)
    generation.setError('')
    try {
      for (const path of paths) {
        const base = path.split(/[/\\]/).pop() ?? 'file'
        const title = base.replace(/\.[^.]+$/, '') || base
        await api.importMediaArtifact(workbenchOrigin('assets'), path, title)
      }
      generation.refresh()
    } catch (err) {
      generation.setError(err instanceof Error ? err.message : String(err))
      generation.refresh()
    } finally {
      setImporting(false)
    }
  }

  async function importLocal() {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Media', extensions: IMPORT_EXTENSIONS }],
    })
    if (picked == null) return
    await importPaths(Array.isArray(picked) ? picked : [picked])
  }

  const zone = useRef<HTMLDivElement>(null)
  const over = useFileDrop(zone, IMPORT_EXTENSIONS, (accepted, rejected) => {
    if (accepted.length > 0) void importPaths(accepted)
    else if (rejected.length > 0) generation.setError(`${t.workbenchDropUnsupported}${IMPORT_EXTENSIONS.join(' / ')}`)
  }, importing)

  async function remove(id: string) {
    generation.setError('')
    try {
      await api.deleteMediaTask(id)
      generation.refresh()
    } catch (err) {
      generation.setError(err instanceof Error ? err.message : String(err))
    }
  }

  async function openFile(path: string) {
    try {
      await api.openLocalFile(path)
    } catch (err) {
      generation.setError(err instanceof Error ? err.message : String(err))
    }
  }

  async function revealFile(path: string) {
    try {
      await api.revealGeneratedFile(path)
    } catch (err) {
      generation.setError(err instanceof Error ? err.message : String(err))
    }
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupContent}
      title={t.workbenchNavAssets}
      subtitle={t.workbenchAssetsSubtitle}
      error={generation.error}
      onErrorDismiss={() => generation.setError('')}
      actions={(
        <>
          <Button size="sm" disabled={importing} onClick={() => void importLocal()}>{t.workbenchAssetsUpload}</Button>
          <Button size="sm" onClick={generation.refresh}>{t.workbenchAssetsRefresh}</Button>
        </>
      )}
    >
      <div ref={zone} className={`workbench-drop-zone flex min-h-0 flex-1 flex-col${over ? ' is-drop-over' : ''}`}>
      <WorkbenchCard fill>
        <div className="workbench-toolbar">
          <div className="workbench-tabs workbench-tabs--scroll">
            {ASSET_TABS.map((item) => {
              const count = item.id === 'all' ? completed.length : completed.filter((task) => task.kind === item.id).length
              return (
                <button
                  key={item.id}
                  type="button"
                  className={`workbench-tab${query.kind === item.id ? ' is-active' : ''}`}
                  onClick={() => setQuery((current) => ({ ...current, kind: item.id }))}
                >
                  {t[item.label]}
                  <span className="workbench-tab-count">{count}</span>
                </button>
              )
            })}
          </div>
          <Select
            value={query.origin}
            ariaLabel={t.workbenchAssetsOrigin}
            onChange={(origin) => setQuery((current) => ({ ...current, origin }))}
            options={[
              { value: '', label: t.workbenchAssetsOriginAll },
              ...origins.map((origin) => ({ value: origin, label: origin })),
            ]}
          />
          <input
            className="workbench-search"
            type="search"
            value={query.keyword}
            aria-label={t.workbenchAssetsSearch}
            placeholder={t.workbenchAssetsSearch}
            onChange={(event) => setQuery((current) => ({ ...current, keyword: event.target.value }))}
          />
        </div>
        {generation.loading && generation.tasks.length === 0 ? <p className="workbench-page-sub">{t.workbenchAssetsLoading}</p> : shown.length === 0 ? (
          <WorkbenchEmpty icon={<FolderOpen size={22} />} title={query.keyword || query.origin || query.kind !== 'all' ? t.workbenchAssetsNoMatch : t.workbenchAssetsEmpty}>
            {t.workbenchAssetsEmptyHint}
          </WorkbenchEmpty>
        ) : (
          <AssetCards tasks={shown} onOpen={(path) => void openFile(path)} onReveal={(path) => void revealFile(path)} onDelete={(id) => void remove(id)} />
        )}
      </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
