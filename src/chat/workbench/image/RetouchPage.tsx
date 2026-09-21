import { useState } from 'react'
import { Sparkles } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { IMAGE_RETOUCH, sizeForImageRatio, type ImageRatioId, type ImageRetouchId } from './imageCatalog'

/**
 * 产品精修：一张参考图 + 模板。开源版还没接出图。
 */
export function RetouchPage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [template, setTemplate] = useState<ImageRetouchId>('white')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [notice, setNotice] = useState('')
  const templateMeta = IMAGE_RETOUCH.find((item) => item.id === template) ?? IMAGE_RETOUCH[0]

  return (
    <ImageStudio
      crumbCurrent={t.workbenchRetouchCrumb}
      title={t.workbenchRetouchTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t[templateMeta.name]}</span>
          <span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>
        </>
      )}
      configTitle={t.workbenchImageConfig}
      configHint={t.workbenchRetouchHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchRetouchUpload}
            required
            hint={t.workbenchImageFileHint}
            max={1}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <div className="workbench-field">
            <span>{t.workbenchRetouchPick}</span>
            <div className="workbench-tile-grid">
              {IMAGE_RETOUCH.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`workbench-tile${template === item.id ? ' is-active' : ''}`}
                  onClick={() => setTemplate(item.id)}
                >
                  <span className="workbench-tile-name">{t[item.name]}</span>
                  <span className="workbench-tile-desc">{t[item.desc]}</span>
                </button>
              ))}
            </div>
          </div>
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
        </>
      )}
      resultTitle={t.workbenchImagePreview}
      resultHint={t.workbenchRetouchResultHint}
      emptyIcon={<Sparkles size={22} />}
      emptyTitle={t.workbenchRetouchEmpty}
      emptyHint={t.workbenchRetouchEmptyHint}
      notice={notice}
      cta={t.workbenchRetouchGenerate}
      onGenerate={() => setNotice(files.length === 0 ? t.workbenchImageNeedProduct : t.workbenchImageSoon)}
    />
  )
}
