import { useRef, useState } from 'react'
import { open } from '@tauri-apps/plugin-dialog'
import { Captions } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select } from '../../../settings/public/controls'
import { MediaTaskList } from '../MediaTaskList'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { SUBTITLE_LANGUAGES, buildSubtitleRequest } from './localEditContract'
import { VIDEO_EXTENSIONS, useFileDrop } from '../useFileDrop'
import { isTauriRuntime } from '../../../api/tauri'

/**
 * 视频字幕处理：本机视频 → local/ffmpeg-subtitle（kind edit）。
 * 识别语音写出 SRT，可选烧录。记录走 MediaTaskList。
 */
export function SubtitlePage() {
  const t = useT()
  const native = isTauriRuntime()
  const generation = useMediaGeneration({ origin: workbenchOrigin('subs') })
  const [video, setVideo] = useState('')
  const [language, setLanguage] = useState('zh')
  const [burn, setBurn] = useState(false)
  const name = video.split(/[\\/]/).pop() || ''
  const zone = useRef<HTMLDivElement>(null)
  const over = useFileDrop(zone, VIDEO_EXTENSIONS, (accepted, rejected) => {
    if (accepted[0]) {
      setVideo(accepted[0])
      generation.setError('')
    } else if (rejected.length > 0) {
      generation.setError(`${t.workbenchDropUnsupported}${VIDEO_EXTENSIONS.join(' / ')}`)
    }
  }, generation.busy)

  async function pick() {
    const path = await open({ filters: [{ name: '视频', extensions: ['mp4', 'mov', 'webm', 'mkv', 'm4v'] }] })
    if (typeof path === 'string') {
      setVideo(path)
      generation.setError('')
    }
  }

  function submit() {
    try {
      void generation.submit(buildSubtitleRequest({ video, language, burn }))
    } catch (failure) {
      generation.setError(failure instanceof Error ? failure.message : String(failure))
    }
  }

  return (
    <WorkbenchPage
      fill
      crumb={t.workbenchGroupVideo}
      crumbCurrent={t.workbenchSubsCrumb}
      title={t.workbenchSubsTitle}
      error={generation.error}
      onErrorDismiss={() => generation.setError('')}
      actions={<span className="workbench-capsule" title={video}>{name || t.workbenchSubsWait}</span>}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchSubsPick} hint={t.workbenchSubsPickHint}>
          <div ref={zone} className={`workbench-field workbench-drop-zone${over ? ' is-drop-over' : ''}`}>
            <span>
              {t.workbenchSubsVideo}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <div className="vs-drop-bar">
              <p className="workbench-page-sub [overflow-wrap:anywhere]">{video || t.workbenchSubsVideoHint}</p>
              <Button size="sm" disabled={!native || generation.busy} onClick={() => void pick()}>{t.workbenchSubsPickFile}</Button>
            </div>
          </div>
          <label className="workbench-field">
            <span>
              {t.workbenchSubsLanguage}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <Select
              className="w-full"
              ariaLabel={t.workbenchSubsLanguage}
              value={language}
              disabled={generation.busy}
              options={SUBTITLE_LANGUAGES.map(([value, label]) => ({ value, label }))}
              onChange={setLanguage}
            />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchSubsBurn}</span>
            <input type="checkbox" checked={burn} disabled={generation.busy} onChange={event => setBurn(event.target.checked)} />
          </label>
          <WorkbenchCta>
            <Button variant="primary" disabled={!native || generation.busy || generation.loading} onClick={submit}>
              {generation.busy ? t.workbenchSubsSubmitting : t.workbenchSubsGenerate}
            </Button>
          </WorkbenchCta>
        </WorkbenchCard>
        <WorkbenchCard fill title={t.workbenchSubsResult} hint={t.workbenchSubsResultHint}>
          {generation.tasks.length === 0 && !generation.loading
            ? <WorkbenchEmpty icon={<Captions size={22} />} title={t.workbenchSubsEmpty}>{t.workbenchSubsEmptyHint}</WorkbenchEmpty>
            : <MediaTaskList bare generation={generation} alt={t.workbenchSubsTitle} />}
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
