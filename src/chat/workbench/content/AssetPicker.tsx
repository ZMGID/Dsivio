import { useEffect, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import type { MediaOutput, MediaTask } from '../../../generated/mediaGeneration'
import { useMediaGeneration } from '../useMediaGeneration'
import { TemplateDialog } from './TemplateDialog'
import { readAssetText } from './assetFiles'
import { assetTitle, filterAssets } from './assetLibrary'
import './assetLibrary.css'

export type AssetPickKind = 'image' | 'text'

/** One chosen library file. `text` is the markdown body for text and null for images. */
export type AssetPick = {
  id: string
  kind: AssetPickKind
  path: string
  mime: string
  text: string | null
  title: string
  origin: string | null
}

export type AssetPickerProps = {
  /** Which completed outputs can be chosen. */
  accept: AssetPickKind[]
  /** Default false: one file. */
  multiple?: boolean
  /** Output paths already chosen by the caller. */
  selectedPaths?: string[]
  onPick: (assets: AssetPick[]) => void
  onClose: () => void
}

function AssetPreview({ task, output }: { task: MediaTask; output: MediaOutput }) {
  const src = convertFileSrc(output.path)
  const [body, setBody] = useState<string | null>(null)
  const [error, setError] = useState('')
  const textual = task.kind === 'text' || output.mime.startsWith('text/') || output.mime === 'application/x-subrip'
  useEffect(() => {
    if (!textual) return
    let active = true
    readAssetText(output.path).then(
      (text) => { if (active) setBody(text) },
      (err) => { if (active) setError(err instanceof Error ? err.message : String(err)) },
    )
    return () => { active = false }
  }, [output.path, textual])
  if (output.mime.startsWith('image/')) return <img className="workbench-asset-preview" src={src} alt={assetTitle(task)} />
  if (output.mime.startsWith('video/')) return <video className="workbench-asset-preview" src={src} controls preload="metadata" />
  if (output.mime.startsWith('audio/')) return <audio className="max-w-full" src={src} controls preload="metadata" aria-label={assetTitle(task)} />
  if (error) return <p className="workbench-inline-note" role="alert">{error}</p>
  if (textual && body == null) return <p className="workbench-page-sub">…</p>
  if (textual && body != null) return <pre className="workbench-asset-text custom-scrollbar">{body.slice(0, 600)}</pre>
  return <p className="workbench-page-sub workbench-page-sub--flush">{output.mime}</p>
}

export function AssetCards({
  tasks,
  selectedPaths,
  onToggle,
  onOpen,
  onReveal,
  onDelete,
}: {
  tasks: MediaTask[]
  selectedPaths?: string[]
  onToggle?: (path: string) => void
  onOpen?: (path: string) => void
  onReveal?: (path: string) => void
  onDelete?: (id: string) => void
}) {
  const t = useT()
  return (
    <div className="workbench-asset-grid">
      {tasks.map((task) => {
        const title = assetTitle(task)
        const primary = task.outputs[0]?.path ?? ''
        const selected = task.outputs.some((output) => selectedPaths?.includes(output.path))
        const body = (
          <>
            <span className="workbench-tile-name">{title}</span>
            {task.prompt ? <span className="workbench-tile-desc">{task.prompt}</span> : null}
            {task.outputs.map((output) => (
              <div key={output.path}>
                <AssetPreview task={task} output={output} />
                {(onOpen || onReveal) && (
                  <div className="workbench-asset-actions">
                    {onOpen ? (
                      <Button size="sm" aria-label={`${t.workbenchAssetsOpen} ${output.path}`} onClick={() => onOpen(output.path)}>
                        {t.workbenchAssetsOpen}
                      </Button>
                    ) : null}
                    {onReveal ? (
                      <Button size="sm" aria-label={`${t.workbenchAssetsReveal} ${output.path}`} onClick={() => onReveal(output.path)}>
                        {t.workbenchAssetsReveal}
                      </Button>
                    ) : null}
                  </div>
                )}
              </div>
            ))}
            <span className="workbench-page-sub workbench-page-sub--flush">{task.origin || task.kind}</span>
            {onDelete ? (
              <div className="workbench-asset-actions">
                <Button size="sm" variant="danger" aria-label={`${t.workbenchAssetsDelete} ${title}`} onClick={() => onDelete(task.id)}>
                  {t.workbenchAssetsDelete}
                </Button>
              </div>
            ) : null}
          </>
        )
        if (onToggle) {
          return (
            <button
              key={task.id}
              type="button"
              className={`workbench-asset-card${selected ? ' is-selected' : ''}`}
              aria-pressed={selected}
              onClick={() => onToggle(primary)}
            >
              {body}
            </button>
          )
        }
        return <article key={task.id} className="workbench-asset-card">{body}</article>
      })}
    </div>
  )
}

/**
 * Chooser for listing and publish forms.
 * `accept` limits kinds. `onPick` receives absolute paths; text picks also include the markdown body.
 */
export function AssetPicker({ accept, multiple = false, selectedPaths, onPick, onClose }: AssetPickerProps) {
  const t = useT()
  const generation = useMediaGeneration({})
  const [kind, setKind] = useState<'all' | AssetPickKind>('all')
  const [keyword, setKeyword] = useState('')
  const [chosen, setChosen] = useState<string[]>(selectedPaths ?? [])
  const [confirming, setConfirming] = useState(false)
  const [localError, setLocalError] = useState('')
  const tabs = (['all', ...accept] as const).filter((item, index, all) => all.indexOf(item) === index)
  const shown = filterAssets(generation.tasks, { kind, origin: '', keyword })
    .filter((task) => accept.includes(task.kind as AssetPickKind))

  function toggle(path: string) {
    if (!path) return
    setChosen((current) => {
      if (!multiple) return current.length === 1 && current[0] === path ? [] : [path]
      return current.includes(path) ? current.filter((item) => item !== path) : [...current, path]
    })
  }

  async function confirm() {
    setConfirming(true)
    setLocalError('')
    try {
      const pool = filterAssets(generation.tasks, { kind: 'all', origin: '', keyword: '' })
      const assets: AssetPick[] = []
      for (const path of chosen) {
        const task = pool.find((item) => item.outputs.some((output) => output.path === path))
        const output = task?.outputs.find((item) => item.path === path)
        if (!task || !output || (task.kind !== 'image' && task.kind !== 'text')) continue
        if (!accept.includes(task.kind)) continue
        const text = task.kind === 'text' ? await readAssetText(output.path) : null
        assets.push({
          id: task.id,
          kind: task.kind,
          path: output.path,
          mime: output.mime,
          text,
          title: assetTitle(task),
          origin: task.origin,
        })
      }
      if (assets.length === 0) {
        setLocalError(t.workbenchAssetsEmpty)
        return
      }
      onPick(assets)
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err))
    } finally {
      setConfirming(false)
    }
  }

  const alert = localError || generation.error
  return (
    <TemplateDialog
      title={t.workbenchAssetsPicker}
      closeLabel={t.workbenchAssetsClose}
      onClose={onClose}
      busy={confirming}
      footer={(
        <>
          <Button onClick={onClose}>{t.cancel}</Button>
          <Button variant="primary" disabled={chosen.length === 0 || confirming} onClick={() => void confirm()}>
            {t.workbenchAssetsUse}
          </Button>
        </>
      )}
    >
      <div className="workbench-toolbar">
        {tabs.length > 2 ? (
          <div className="workbench-tabs">
            {tabs.map((item) => (
              <button key={item} type="button" className={`workbench-tab${kind === item ? ' is-active' : ''}`} onClick={() => setKind(item)}>
                {item === 'all' ? t.workbenchAssetsTabAll : item === 'image' ? t.workbenchAssetsTabImage : t.workbenchAssetsTabText}
              </button>
            ))}
          </div>
        ) : null}
        <input
          className="workbench-search"
          type="search"
          value={keyword}
          aria-label={t.workbenchAssetsSearch}
          placeholder={t.workbenchAssetsSearch}
          onChange={(event) => setKeyword(event.target.value)}
        />
      </div>
      {alert ? <p className="workbench-inline-note" role="alert">{alert}</p> : null}
      {generation.loading && shown.length === 0 ? <p className="workbench-page-sub">{t.workbenchAssetsLoading}</p> : shown.length === 0 ? (
        <p className="workbench-page-sub">{keyword ? t.workbenchAssetsNoMatch : t.workbenchAssetsEmpty}</p>
      ) : (
        <AssetCards tasks={shown} selectedPaths={chosen} onToggle={toggle} />
      )}
    </TemplateDialog>
  )
}
