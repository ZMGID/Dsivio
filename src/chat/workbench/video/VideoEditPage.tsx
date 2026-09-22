import { useState } from 'react'
import { Film } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoUploadField } from './VideoUploadField'
import { useLocalVideo } from './useLocalVideo'

/**
 * 产品视频编辑：待编辑视频 + 参考图。开源版还没接处理。
 */
export function VideoEditPage() {
  const t = useT()
  const [video, setVideo] = useLocalVideo()
  const [refs, setRefs] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [notice, setNotice] = useState('')

  const generate = () => {
    if (!video) {
      setNotice(t.workbenchVideoNeedFile)
      return
    }
    if (refs.length === 0) {
      setNotice(t.workbenchImageNeedRef)
      return
    }
    if (!brief.trim()) {
      setNotice(t.workbenchVeditNeedBrief)
      return
    }
    setNotice(t.workbenchVideoSoon)
  }

  return (
    <WorkbenchPage mediaPool="videoModels"
      fill
      crumb={t.workbenchGroupVideo}
      crumbCurrent={t.workbenchVeditCrumb}
      title={t.workbenchVeditTitle}
      actions={<span className="workbench-capsule" title={video?.name}>{video ? video.name : t.workbenchVeditWait}</span>}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchVeditSettings} hint={t.workbenchVeditSettingsHint}>
          <VideoUploadField
            label={t.workbenchVeditVideo}
            required
            hint={t.workbenchVeditVideoHint}
            maxBytes={50 * 1024 * 1024}
            accept="video/mp4,video/quicktime"
            file={video}
            onChange={setVideo}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchVeditRef}
            required
            hint={t.workbenchVeditRefHint}
            max={4}
            files={refs}
            onChange={setRefs}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>
              {t.workbenchVeditBrief}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchVeditBriefHint} />
          </label>
          <WorkbenchCta notice={notice}>
            <Button variant="primary" onClick={generate}>{t.workbenchVeditGenerate}</Button>
          </WorkbenchCta>
        </WorkbenchCard>
        <WorkbenchCard fill title={t.workbenchVeditResult} hint={t.workbenchVeditResultHint}>
          <WorkbenchEmpty icon={<Film size={22} />} title={t.workbenchVeditEmpty}>
            {t.workbenchVeditEmptyHint}
          </WorkbenchEmpty>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
