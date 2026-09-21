import { useRef } from 'react'
import { Plus, X } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { revokeVideo, type LocalVideo } from './useLocalVideo'

/**
 * 本机选一个视频。上限按页传入，默认 40MB。
 */
export function VideoUploadField({
  label,
  required,
  hint,
  maxBytes = 40 * 1024 * 1024,
  accept = 'video/*',
  file,
  onChange,
  onNotice,
}: {
  label: string
  required?: boolean
  hint?: string
  maxBytes?: number
  accept?: string
  file: LocalVideo | null
  onChange: (file: LocalVideo | null) => void
  onNotice: (text: string) => void
}) {
  const t = useT()
  const inputRef = useRef<HTMLInputElement>(null)

  return (
    <div className="workbench-upload">
      <div className="workbench-upload-head">
        <span>
          {label}
          {required ? <span className="workbench-required" aria-hidden="true">*</span> : null}
        </span>
        <Button size="sm" onClick={() => inputRef.current?.click()}>{t.workbenchCopyUpload}</Button>
      </div>
      <div className="workbench-upload-slots">
        <button type="button" className="workbench-upload-slot" onClick={() => inputRef.current?.click()}>
          <Plus size={18} />
        </button>
        {file ? (
          <div className="workbench-upload-slot workbench-upload-slot--filled" title={file.name}>
            <span className="workbench-video-name">{file.name}</span>
            <button
              type="button"
              className="workbench-upload-remove"
              aria-label={t.workbenchUploadRemove}
              onClick={() => {
                revokeVideo(file)
                onChange(null)
              }}
            >
              <X size={12} />
            </button>
          </div>
        ) : null}
      </div>
      {hint ? <p className="workbench-page-sub">{hint}</p> : null}
      <input
        ref={inputRef}
        type="file"
        accept={accept}
        hidden
        onChange={(event) => {
          const next = event.target.files?.[0]
          event.target.value = ''
          if (!next) return
          if (!next.type.startsWith('video/')) return
          if (next.size > maxBytes) {
            onNotice(t.workbenchVideoTooBig)
            return
          }
          onNotice('')
          revokeVideo(file)
          onChange({ id: crypto.randomUUID(), name: next.name, url: URL.createObjectURL(next) })
        }}
      />
    </div>
  )
}
