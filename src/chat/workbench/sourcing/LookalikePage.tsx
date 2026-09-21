import { useCallback, useEffect, useRef, useState } from 'react'
import { Clock, ImagePlus } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'

const HISTORY_KEY = 'kivio.workbench.matchHistory'
const MAX_BYTES = 10 * 1024 * 1024

type HistoryItem = { id: string; name: string; at: string }

function loadHistory(): HistoryItem[] {
  try {
    const raw = window.localStorage.getItem(HISTORY_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return []
    return parsed.filter((item): item is HistoryItem => (
      typeof item === 'object' && item !== null
      && typeof (item as HistoryItem).id === 'string'
      && typeof (item as HistoryItem).name === 'string'
      && typeof (item as HistoryItem).at === 'string'
    ))
  } catch {
    return []
  }
}

function saveHistory(items: HistoryItem[]): void {
  window.localStorage.setItem(HISTORY_KEY, JSON.stringify(items.slice(0, 20)))
}

/**
 * 同款找货：上传图、搜 1688。图可以先落在本机；货源接口还没接。
 */
export function LookalikePage() {
  const t = useT()
  const fileRef = useRef<HTMLInputElement>(null)
  const [fileName, setFileName] = useState('')
  const [preview, setPreview] = useState('')
  const [query, setQuery] = useState('')
  const [sort, setSort] = useState('default')
  const [historyOpen, setHistoryOpen] = useState(false)
  const [history, setHistory] = useState<HistoryItem[]>(loadHistory)
  const [notice, setNotice] = useState('')

  useEffect(() => () => {
    if (preview) URL.revokeObjectURL(preview)
  }, [preview])

  const pickFile = useCallback((file: File | undefined) => {
    if (!file) return
    if (file.size > MAX_BYTES) {
      setNotice(t.workbenchMatchTooBig)
      return
    }
    setFileName(file.name)
    setNotice(t.workbenchMatchSoon)
    setPreview((current) => {
      if (current) URL.revokeObjectURL(current)
      return URL.createObjectURL(file)
    })
    const item = { id: crypto.randomUUID(), name: file.name, at: new Date().toISOString() }
    const next = [item, ...history.filter((row) => row.name !== file.name)]
    setHistory(next)
    saveHistory(next)
  }, [history, t])

  useEffect(() => {
    const onPaste = (event: ClipboardEvent) => {
      const file = [...event.clipboardData?.items ?? []]
        .find((item) => item.type.startsWith('image/'))
        ?.getAsFile()
      if (file) pickFile(file)
    }
    window.addEventListener('paste', onPaste)
    return () => window.removeEventListener('paste', onPaste)
  }, [pickFile])

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupSourcing}
      title={t.workbenchNavMatch}
      subtitle={t.workbenchMatchSubtitle}
      actions={(
        <div className="workbench-page-actions">
          <Button size="sm" variant={historyOpen ? 'primary' : 'default'} onClick={() => setHistoryOpen((open) => !open)}>
            <Clock size={14} />
            {t.workbenchMatchHistory}
          </Button>
          {historyOpen ? (
            <div className="workbench-history-pop">
              <p className="workbench-card-block-title">{t.workbenchMatchRecent}</p>
              <p className="workbench-page-sub workbench-page-sub--flush">{t.workbenchMatchRecentHint}</p>
              {history.length === 0 ? (
                <WorkbenchEmpty>{t.workbenchMatchHistoryEmpty}</WorkbenchEmpty>
              ) : (
                <ul className="workbench-history-list">
                  {history.map((item) => (
                    <li key={item.id}>
                      <button type="button" className="workbench-tab" onClick={() => setFileName(item.name)}>
                        {item.name}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : null}
        </div>
      )}
    >
      <section
        className="workbench-card-block workbench-dropzone"
        tabIndex={0}
        onClick={() => fileRef.current?.click()}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault()
          pickFile(event.dataTransfer.files[0])
        }}
        onPaste={(event) => {
          const file = [...event.clipboardData.items]
            .find((item) => item.type.startsWith('image/'))
            ?.getAsFile()
          pickFile(file ?? undefined)
        }}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault()
            fileRef.current?.click()
          }
        }}
      >
        <div className="workbench-dropzone-inner">
          {preview ? (
            <img className="workbench-dropzone-preview" src={preview} alt={fileName} />
          ) : (
            <div className="workbench-empty-icon"><ImagePlus size={22} /></div>
          )}
          <h2 className="workbench-dropzone-title">{t.workbenchMatchUploadTitle}</h2>
          <p className="workbench-page-sub workbench-page-sub--flush">{t.workbenchMatchUploadHint}</p>
          <Button
            size="sm"
            onClick={(event) => {
              event.stopPropagation()
              fileRef.current?.click()
            }}
          >
            {t.workbenchMatchUpload}
          </Button>
          {fileName || notice ? (
            <p className="workbench-inline-note">{notice || fileName}</p>
          ) : null}
        </div>
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          hidden
          onChange={(event) => {
            pickFile(event.target.files?.[0])
            event.target.value = ''
          }}
        />
      </section>

      <WorkbenchCard
        fill
        title={t.workbenchMatchEmpty}
        extra={(
          <div className="workbench-toolbar">
            <input
              className="workbench-search workbench-search--narrow"
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t.workbenchMatchFilter}
            />
            <Select
              value={sort}
              onChange={setSort}
              ariaLabel={t.workbenchMatchSort}
              options={[{ value: 'default', label: t.workbenchMatchSort }]}
            />
          </div>
        )}
      >
        <WorkbenchEmpty />
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
