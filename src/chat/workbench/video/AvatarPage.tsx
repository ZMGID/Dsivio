import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoParamLine, VideoStudio } from './VideoStudio'

/**
 * 真人带货：产品图 + 角色参考图。开源版还没接出片。
 */
export function AvatarPage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [prompt, setPrompt] = useState('')
  const [notice, setNotice] = useState('')

  const helpWrite = () => {
    setNotice(brief.trim() ? t.workbenchVideoHelpSoon : t.workbenchAvatarNeedBrief)
  }

  const generate = () => {
    if (files.length === 0) {
      setNotice(t.workbenchImageNeedProduct)
      return
    }
    if (!brief.trim()) {
      setNotice(t.workbenchAvatarNeedBrief)
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
      crumbCurrent={t.workbenchAvatarCrumb}
      title={t.workbenchAvatarTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t.workbenchVideoRes768}</span>
          <span className="workbench-capsule">{t.workbenchVideoDur10}</span>
        </>
      )}
      settingsTitle={t.workbenchShortsSettings}
      settingsHint={t.workbenchAvatarSettingsHint}
      settings={(
        <>
          <CopyUploadField
            label={t.workbenchAvatarProduct}
            required
            max={3}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchAvatarRole}
            optional
            hint=""
            max={2}
            files={roles}
            onChange={setRoles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>
              {t.workbenchAvatarBrief}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchAvatarBriefHint} />
          </label>
        </>
      )}
      generateTitle={t.workbenchVideoGenerate}
      generateHint={t.workbenchAvatarGenerateHint}
      generate={(
        <>
          <VideoParamLine items={[t.workbenchVideoRes768, t.workbenchVideoSize916, t.workbenchVideoDur10, t.workbenchImageZh]} />
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
      taskHint={t.workbenchAvatarTaskHint}
      taskEmpty={t.workbenchAvatarTaskEmpty}
    />
  )
}
