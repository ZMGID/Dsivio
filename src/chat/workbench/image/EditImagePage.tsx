import { useState } from 'react'
import { Pencil } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField, revokeImages } from '../copy/CopyUploadField'
import { ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { sizeForImageRatio, type ImageRatioId } from './imageCatalog'

type EditTab = 'single' | 'multi'
type EditMode = 'fast' | 'rich'

/**
 * 图片编辑：上传 + 指令。框选改图还没接，多图只改上传张数。
 */
export function EditImagePage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [tab, setTab] = useState<EditTab>('single')
  const [mode, setMode] = useState<EditMode>('fast')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [notice, setNotice] = useState('')

  const generate = () => {
    if (files.length === 0) {
      setNotice(t.workbenchImageNeedProduct)
      return
    }
    if (!brief.trim()) {
      setNotice(t.workbenchEditNeedBrief)
      return
    }
    setNotice(t.workbenchImageSoon)
  }

  return (
    <ImageStudio
      crumbCurrent={t.workbenchEditCrumb}
      title={t.workbenchEditTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{tab === 'single' ? t.workbenchEditCurrent : t.workbenchEditMulti}</span>
          <span className="workbench-capsule">{t.workbenchEditResultCount} 0</span>
        </>
      )}
      tabs={(
        <div className="workbench-tabs">
          <button
            type="button"
            className={`workbench-tab${tab === 'single' ? ' is-active' : ''}`}
            onClick={() => {
              setTab('single')
              if (files.length > 1) {
                revokeImages(files.slice(1))
                setFiles(files.slice(0, 1))
              }
            }}
          >
            {t.workbenchEditSingle}
          </button>
          <button type="button" className={`workbench-tab${tab === 'multi' ? ' is-active' : ''}`} onClick={() => setTab('multi')}>
            {t.workbenchEditMulti}
          </button>
        </div>
      )}
      configTitle={t.workbenchEditUpload}
      configHint={t.workbenchEditUploadHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchEditUpload}
            required
            hint={t.workbenchImageFileHint}
            max={tab === 'single' ? 1 : 4}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>{t.workbenchEditBrief}</span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchEditBriefHint} />
          </label>
          <p className="workbench-page-sub">{t.workbenchEditBriefHint2}</p>
          <div className="workbench-field">
            <span>{t.workbenchEditMode}</span>
            <div className="workbench-tile-grid">
              <button type="button" className={`workbench-tile${mode === 'fast' ? ' is-active' : ''}`} onClick={() => setMode('fast')}>
                <span className="workbench-tile-name">{t.workbenchEditFast}</span>
                <span className="workbench-tile-desc">{t.workbenchEditFastDesc}</span>
              </button>
              <button type="button" className={`workbench-tile${mode === 'rich' ? ' is-active' : ''}`} onClick={() => setMode('rich')}>
                <span className="workbench-tile-name">{t.workbenchEditRich}</span>
                <span className="workbench-tile-desc">{t.workbenchEditRichDesc}</span>
              </button>
            </div>
          </div>
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
        </>
      )}
      resultTitle={t.workbenchEditResult}
      resultHint={t.workbenchEditResultHint}
      resultExtra={<span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>}
      emptyIcon={<Pencil size={22} />}
      emptyTitle={t.workbenchEditEmpty}
      emptyHint={t.workbenchEditEmptyHint}
      notice={notice}
      cta={t.workbenchEditGenerate}
      onGenerate={generate}
    />
  )
}
