import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoParamLine, VideoStudio } from './VideoStudio'

/**
 * 带货短视频：产品图 + 描述 + 提示词。开源版还没接出片。
 */
export function ShortsPage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [prompt, setPrompt] = useState('')
  const [notice, setNotice] = useState('')

  const helpWrite = () => {
    setNotice(brief.trim() ? t.workbenchVideoHelpSoon : t.workbenchShortsNeedBrief)
  }

  const generate = () => {
    if (files.length === 0) {
      setNotice(t.workbenchImageNeedProduct)
      return
    }
    if (!brief.trim()) {
      setNotice(t.workbenchShortsNeedBrief)
      return
    }
    if (!prompt.trim()) {
      setNotice(t.workbenchVideoNeedPrompt)
      return
    }
    setNotice(t.workbenchVideoSoon)
  }

  return (
    <VideoStudio
      crumbCurrent={t.workbenchShortsCrumb}
      title={t.workbenchShortsTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t.workbenchVideoRes1080}</span>
          <span className="workbench-capsule">{t.workbenchVideoSize916}</span>
          <span className="workbench-capsule">{t.workbenchVideoDur5}</span>
        </>
      )}
      settingsTitle={t.workbenchShortsSettings}
      settingsHint={t.workbenchShortsSettingsHint}
      settings={(
        <>
          <CopyUploadField
            label={t.workbenchShortsProduct}
            required
            max={6}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>
              {t.workbenchShortsBrief}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchShortsBriefHint} />
          </label>
          <p className="workbench-page-sub">{t.workbenchShortsBriefHelp}</p>
          <CopyUploadField
            label={t.workbenchVideoRole}
            optional
            hint={t.workbenchVideoRoleHint}
            max={3}
            files={roles}
            onChange={setRoles}
            onNotice={setNotice}
          />
        </>
      )}
      generateTitle={t.workbenchVideoGenerate}
      generateHint={t.workbenchShortsGenerateHint}
      generate={(
        <>
          <VideoParamLine items={[t.workbenchVideoRes1080, t.workbenchVideoSize916, t.workbenchVideoDur5, t.workbenchImageZh]} />
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>
                {t.workbenchVideoPrompt}
                <span className="workbench-required" aria-hidden="true">*</span>
              </span>
              <Button size="sm" onClick={helpWrite}>{t.workbenchVideoHelpWrite}</Button>
            </span>
            <TextArea value={prompt} onChange={setPrompt} rows={6} placeholder={t.workbenchVideoPromptHint} />
          </label>
        </>
      )}
      notice={notice}
      cta={t.workbenchVideoGenerateCta}
      onGenerate={generate}
      taskHint={t.workbenchShortsTaskHint}
      taskEmpty={t.workbenchShortsTaskEmpty}
    />
  )
}
