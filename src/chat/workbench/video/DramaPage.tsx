import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoParamLine, VideoStudio } from './VideoStudio'
import { DRAMA_STYLES, type DramaStyleId } from './videoCatalog'

/**
 * 短剧带货：产品图 + 风格 + 剧本。开源版还没接出片。
 */
export function DramaPage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [prompt, setPrompt] = useState('')
  const [style, setStyle] = useState<DramaStyleId>('twist')
  const [notice, setNotice] = useState('')
  const styleMeta = DRAMA_STYLES.find((item) => item.id === style) ?? DRAMA_STYLES[0]

  const writeScript = () => {
    setNotice(brief.trim() ? t.workbenchDramaScriptSoon : t.workbenchDramaNeedBrief)
  }

  const generate = () => {
    if (files.length === 0) {
      setNotice(t.workbenchImageNeedProduct)
      return
    }
    if (!brief.trim()) {
      setNotice(t.workbenchDramaNeedBrief)
      return
    }
    if (!prompt.trim()) {
      setNotice(t.workbenchDramaNeedScript)
      return
    }
    setNotice(t.workbenchVideoSoon)
  }

  return (
    <VideoStudio
      crumbCurrent={t.workbenchDramaCrumb}
      title={t.workbenchDramaTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t[styleMeta.label]}</span>
          <span className="workbench-capsule">{t.workbenchVideoDur15}</span>
        </>
      )}
      settingsTitle={t.workbenchDramaSettings}
      settingsHint={t.workbenchDramaSettingsHint}
      settings={(
        <>
          <CopyUploadField
            label={t.workbenchDramaProduct}
            required
            max={4}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>
              {t.workbenchDramaBrief}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchDramaBriefHint} />
          </label>
          <CopyUploadField
            label={t.workbenchDramaRole}
            optional
            hint={t.workbenchDramaRoleHint}
            max={6}
            files={roles}
            onChange={setRoles}
            onNotice={setNotice}
          />
        </>
      )}
      generateTitle={t.workbenchVideoGenerate}
      generateHint={t.workbenchDramaGenerateHint}
      generate={(
        <>
          <div className="workbench-field">
            <span>{t.workbenchDramaStyle}</span>
            <div className="workbench-chip-row">
              {DRAMA_STYLES.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`workbench-chip${style === item.id ? ' is-active' : ''}`}
                  onClick={() => setStyle(item.id)}
                >
                  {t[item.label]}
                </button>
              ))}
            </div>
          </div>
          <VideoParamLine items={[t.workbenchVideoRes1080, t.workbenchVideoSize916, t.workbenchVideoDur15, t.workbenchImageZh]} />
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>
                {t.workbenchVideoPrompt}
                <span className="workbench-required" aria-hidden="true">*</span>
              </span>
              <Button size="sm" onClick={writeScript}>{t.workbenchDramaWriteScript}</Button>
            </span>
            <TextArea value={prompt} onChange={setPrompt} rows={6} placeholder={t.workbenchDramaPromptHint} />
          </label>
        </>
      )}
      notice={notice}
      cta={t.workbenchVideoGenerateCta}
      onGenerate={generate}
      taskHint={t.workbenchDramaTaskHint}
      taskEmpty={t.workbenchDramaTaskEmpty}
    />
  )
}
