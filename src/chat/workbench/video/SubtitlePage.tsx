import { useState } from 'react'
import { Captions } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
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
      actions={<span className="workbench-capsule" title={video?.name}>{video ? video.name : t.workbenchSubsWait}</span>}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchSubsPick} hint={t.workbenchSubsPickHint}>
          <VideoUploadField
            label={t.workbenchSubsVideo}
            required
            hint={t.workbenchSubsVideoHint}
            file={video}
            onChange={setVideo}
            onNotice={setNotice}
          />
          <WorkbenchCta notice={notice}>
            <Button
              variant="primary"
              onClick={() => setNotice(video ? t.workbenchSubsSoon : t.workbenchVideoNeedFile)}
            >
              {t.workbenchSubsGenerate}
            </Button>
          </WorkbenchCta>
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
