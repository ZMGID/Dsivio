import { useState } from 'react'
import { PanelsTopLeft } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { IMAGE_COVERS, sizeForImageRatio, type ImageCoverId, type ImageRatioId } from './imageCatalog'

/**
 * 海报封面：需求描述 + 封面类型。开源版还没接出图。
 */
export function PosterPage() {
  const t = useT()
  const [refs, setRefs] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [cover, setCover] = useState<ImageCoverId>('brand')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [notice, setNotice] = useState('')
  const coverMeta = IMAGE_COVERS.find((item) => item.id === cover) ?? IMAGE_COVERS[0]
  const size = sizeForImageRatio(ratio)

  return (
    <ImageStudio
      crumbCurrent={t.workbenchPosterCrumb}
      title={t.workbenchPosterTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t[coverMeta.label]}</span>
          <span className="workbench-capsule">{size}</span>
        </>
      )}
      configTitle={t.workbenchImageConfig}
      configHint={t.workbenchPosterHint}
      config={(
        <>
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>
                {t.workbenchPosterBrief}
                <span className="workbench-required" aria-hidden="true">*</span>
              </span>
              <button type="button" className="workbench-tab" onClick={() => setBrief('')}>{t.workbenchCopyClear}</button>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchPosterBriefHint} />
          </label>
          <CopyUploadField
            label={t.workbenchImageRef}
            hint=""
            max={1}
            files={refs}
            onChange={setRefs}
            onNotice={setNotice}
          />
          <div className="workbench-field">
            <span>{t.workbenchPosterType}</span>
            <div className="workbench-tile-grid">
              {IMAGE_COVERS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`workbench-tile${cover === item.id ? ' is-active' : ''}`}
                  onClick={() => setCover(item.id)}
                >
                  <span className="workbench-tile-name">{t[item.label]}</span>
                </button>
              ))}
            </div>
          </div>
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
        </>
      )}
      resultTitle={t.workbenchImagePreview}
      resultHint={t.workbenchPosterResultHint}
      resultExtra={<span className="workbench-capsule">{size}</span>}
      emptyIcon={<PanelsTopLeft size={22} />}
      emptyTitle={t.workbenchPosterEmpty}
      emptyHint={t.workbenchPosterEmptyHint}
      notice={notice}
      cta={t.workbenchPosterGenerate}
      onGenerate={() => setNotice(brief.trim() ? t.workbenchImageSoon : t.workbenchPosterNeedBrief)}
    />
  )
}
