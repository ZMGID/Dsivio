import { useState } from 'react'
import { Captions } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { VideoUploadField } from './VideoUploadField'
import { useLocalVideo } from './useLocalVideo'

/**
 * 字幕去除：本机选视频。开源版还没接擦除。
 */
export function SubtitlePage() {
  const t = useT()
  const [video, setVideo] = useLocalVideo()
  const [notice, setNotice] = useState('')

  return (
    <WorkbenchPage
      fill
      crumb={t.workbenchGroupVideo}
      crumbCurrent={t.workbenchSubsCrumb}
      title={t.workbenchSubsTitle}
      actions={<span className="workbench-capsule">{video ? video.name : t.workbenchSubsWait}</span>}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchSubsPick} hint={t.workbenchSubsPickHint}>
          <div className="workbench-card-scroll custom-scrollbar">
            <VideoUploadField
              label={t.workbenchSubsVideo}
              required
              hint={t.workbenchSubsVideoHint}
              file={video}
              onChange={setVideo}
              onNotice={setNotice}
            />
            {notice ? <p className="workbench-inline-note">{notice}</p> : null}
          </div>
          <div className="workbench-cta-row">
            <Button
              variant="primary"
              onClick={() => setNotice(video ? t.workbenchSubsSoon : t.workbenchVideoNeedFile)}
            >
              {t.workbenchSubsGenerate}
            </Button>
          </div>
        </WorkbenchCard>
        <WorkbenchCard fill title={t.workbenchSubsResult} hint={t.workbenchSubsResultHint}>
          <WorkbenchEmpty icon={<Captions size={22} />} title={t.workbenchSubsEmpty}>
            {t.workbenchSubsEmptyHint}
          </WorkbenchEmpty>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
