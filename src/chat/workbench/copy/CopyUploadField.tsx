import { useEffect, useRef } from 'react'
import { Plus, X } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { imageFromPath, revokeImages, type LocalImage } from '../localMedia'
import { IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'

const DEFAULT_MAX = 7
const MAX_BYTES = 10 * 1024 * 1024

/**
 * 图文页共用的商品图上传：本机选图，默认最多 7 张。
 */
export function CopyUploadField({
  label,
  required,
  optional,
  hint,
  max = DEFAULT_MAX,
  files,
  onChange,
  onNotice,
}: {
  label: string
  required?: boolean
  optional?: boolean
  hint?: string
  max?: number
  files: LocalImage[]
  onChange: (files: LocalImage[]) => void
  onNotice: (text: string) => void
}) {
  const t = useT()
  const inputRef = useRef<HTMLInputElement>(null)
  const zone = useRef<HTMLDivElement>(null)
  const filesRef = useRef(files)
  filesRef.current = files
  const alive = useRef(true)
  useEffect(() => {
    alive.current = true
    return () => { alive.current = false }
  }, [])

  const addPaths = async (accepted: string[], rejected: string[]) => {
    let notice = rejected.length > 0 ? `${t.workbenchDropUnsupported}${IMAGE_EXTENSIONS.join(' / ')}` : ''
    const added: LocalImage[] = []
    for (const path of accepted) {
      if (filesRef.current.length + added.length >= max) {
        notice = t.workbenchCopyMaxFiles
        break
      }
      try {
        added.push(await imageFromPath(path))
      } catch (error) {
        notice = `${t.workbenchDropFailed}${error instanceof Error ? error.message : String(error)}`
      }
    }
    if (!alive.current) {
      revokeImages(added)
      return
    }
    const kept = added.slice(0, Math.max(max - filesRef.current.length, 0))
    revokeImages(added.slice(kept.length))
    if (kept.length > 0) onChange([...filesRef.current, ...kept])
    onNotice(notice)
  }
  const over = useFileDrop(zone, IMAGE_EXTENSIONS, (accepted, rejected) => { void addPaths(accepted, rejected) })

  const addFiles = (list: FileList | null) => {
    if (!list) return
    const next = [...files]
    for (const file of list) {
      if (!file.type.startsWith('image/')) continue
      if (file.size > MAX_BYTES) {
        onNotice(t.workbenchMatchTooBig)
        return
      }
      if (next.length >= max) {
        onNotice(t.workbenchCopyMaxFiles)
        break
      }
      next.push({ id: crypto.randomUUID(), name: file.name, url: URL.createObjectURL(file) })
    }
    if (next.length !== files.length) onNotice('')
    onChange(next)
  }

  return (
    <div ref={zone} className={`workbench-upload${over ? ' is-drop-over' : ''}`}>
      <div className="workbench-upload-head">
        <span>
          {label}
          {required ? <span className="workbench-required" aria-hidden="true">*</span> : null}
          {optional ? <em>{t.workbenchCopyOptional}</em> : null}
        </span>
        <div className="workbench-upload-row">
          <Button size="sm" onClick={() => inputRef.current?.click()}>{t.workbenchCopyUpload}</Button>
          <span className="workbench-page-sub workbench-page-sub--flush">{files.length}/{max}</span>
        </div>
      </div>
      <div className="workbench-upload-slots">
        <button type="button" className="workbench-upload-slot" onClick={() => inputRef.current?.click()}>
          <Plus size={18} />
        </button>
        {files.map((file) => (
          <div
            key={file.id}
            className="workbench-upload-slot workbench-upload-slot--filled"
            title={file.name}
          >
            <img src={file.url} alt={file.name} />
            <button
              type="button"
              className="workbench-upload-remove"
              aria-label={t.workbenchUploadRemove}
              onClick={() => {
                URL.revokeObjectURL(file.url)
                onChange(files.filter((item) => item.id !== file.id))
              }}
            >
              <X size={12} />
            </button>
          </div>
        ))}
      </div>
      {hint === '' ? null : <p className="workbench-page-sub">{hint ?? t.workbenchCopyUploadHint}</p>}
      <input
        ref={inputRef}
        type="file"
        accept="image/*"
        multiple={max > 1}
        hidden
        onChange={(event) => {
          addFiles(event.target.files)
          event.target.value = ''
        }}
      />
    </div>
  )
}
