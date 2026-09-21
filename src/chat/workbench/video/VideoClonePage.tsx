import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoParamLine, VideoStudio } from './VideoStudio'
import { VideoUploadField } from './VideoUploadField'
import { useLocalVideo } from './useLocalVideo'

/**
 * 视频复刻：先理解原片，再生成。两步都还没接。
 */
export function VideoClonePage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [video, setVideo] = useLocalVideo()
  const [brief, setBrief] = useState('')
  const [prompt, setPrompt] = useState('')
  const [notice, setNotice] = useState('')

  const understand = () => {
    if (!video) {
      setNotice(t.workbenchVideoNeedFile)
      return
    }
    setNotice(t.workbenchVcloneUnderstandSoon)
  }

  const generate = () => {
    if (files.length === 0) {
      setNotice(t.workbenchImageNeedProduct)
      return
    }
    if (!video) {
      setNotice(t.workbenchVideoNeedFile)
      return
    }
    setNotice(t.workbenchVcloneNeedUnderstand)
  }

  return (
    <VideoStudio
      crumbCurrent={t.workbenchVcloneCrumb}
      title={t.workbenchVcloneTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t.workbenchVideoRes1080}</span>
          <span className="workbench-capsule">{t.workbenchVideoDur5}</span>
        </>
      )}
      settingsTitle={t.workbenchShortsSettings}
      settingsHint={t.workbenchVcloneSettingsHint}
      settings={(
        <>
          <CopyUploadField
            label={t.workbenchVcloneProduct}
            required
            max={6}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchVideoRole}
            optional
            hint=""
            max={3}
            files={roles}
            onChange={setRoles}
            onNotice={setNotice}
          />
          <VideoUploadField
            label={t.workbenchVcloneVideo}
            required
            hint={t.workbenchVcloneVideoHint}
            file={video}
            onChange={setVideo}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>{t.workbenchVcloneBrief}</span>
            <TextArea value={brief} onChange={setBrief} rows={4} placeholder={t.workbenchVcloneBriefHint} />
          </label>
        </>
      )}
      generateTitle={t.workbenchVideoGenerate}
      generateHint={t.workbenchVcloneGenerateHint}
      generate={(
        <>
          <VideoParamLine items={[t.workbenchVideoRes1080, t.workbenchVideoSize916, t.workbenchVideoDur5, t.workbenchImageZh]} />
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>
                {t.workbenchVideoPrompt}
                <span className="workbench-required" aria-hidden="true">*</span>
              </span>
              <Button size="sm" onClick={understand}>{t.workbenchVcloneUnderstand}</Button>
            </span>
            <TextArea value={prompt} onChange={setPrompt} rows={6} placeholder={t.workbenchVclonePromptHint} />
          </label>
        </>
      )}
      notice={notice}
      cta={t.workbenchVideoGenerateCta}
      onGenerate={generate}
      taskHint={t.workbenchVcloneTaskHint}
      taskEmpty={t.workbenchVcloneTaskEmpty}
    />
  )
}
