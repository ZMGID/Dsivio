import { useState } from 'react'
import { Image } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageCountField, ImagePlatformLine, ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import {
  IMAGE_STYLES,
  sizeForImageRatio,
  type ImageCount,
  type ImageRatioId,
  type ImageStyleId,
} from './imageCatalog'

/**
 * 主图：产品图 + 风格 + 尺寸。开源版还没接出图，结果区保持空。
 */
export function MainImagePage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [style, setStyle] = useState<ImageStyleId>('white')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [count, setCount] = useState<ImageCount>(4)
  const [notice, setNotice] = useState('')
  const styleMeta = IMAGE_STYLES.find((item) => item.id === style) ?? IMAGE_STYLES[0]

  return (
    <ImageStudio
      crumbCurrent={t.workbenchMainCrumb}
      title={t.workbenchMainTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t[styleMeta.label]}</span>
          <span className="workbench-capsule">{count} {t.workbenchImageSheets}</span>
          <span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>
        </>
      )}
      configTitle={t.workbenchImageConfig}
      configHint={t.workbenchMainHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchImageProduct}
            required
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchImageRole}
            optional
            hint=""
            max={3}
            files={roles}
            onChange={setRoles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>{t.workbenchMainBrief}</span>
            <TextArea value={brief} onChange={setBrief} rows={4} placeholder={t.workbenchMainBriefHint} />
          </label>
          <ImagePlatformLine />
          <p className="workbench-page-sub">{t.workbenchMainPlatformHint}</p>
          <div className="workbench-field">
            <span>{t.workbenchMainStyle}</span>
            <div className="workbench-chip-row">
              {IMAGE_STYLES.map((item) => (
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
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
          <ImageCountField count={count} onCount={setCount} />
        </>
      )}
      resultTitle={t.workbenchImageResult}
      emptyIcon={<Image size={22} />}
      emptyTitle={t.workbenchMainEmpty}
      emptyHint={t.workbenchMainEmptyHint}
      notice={notice}
      cta={t.workbenchMainGenerate}
      onGenerate={() => setNotice(files.length === 0 ? t.workbenchImageNeedProduct : t.workbenchImageSoon)}
    />
  )
}
