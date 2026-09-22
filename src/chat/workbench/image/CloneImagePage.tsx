import { useState } from 'react'
import { Copy } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageCountField, ImageSizeFields } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { sizeForImageRatio, type ImageCount, type ImageRatioId } from './imageCatalog'

/**
 * 图片复刻：先分析参考图，再生成。两步都还没接模型。
 */
export function CloneImagePage() {
  const t = useT()
  const [ref, setRef] = useLocalImages()
  const [products, setProducts] = useLocalImages()
  const [prompt, setPrompt] = useState('')
  const [analyzed, setAnalyzed] = useState(false)
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [count, setCount] = useState<ImageCount>(1)
  const [notice, setNotice] = useState('')

  const analyze = () => {
    if (ref.length === 0) {
      setNotice(t.workbenchImageNeedRef)
      return
    }
    setAnalyzed(false)
    setNotice(t.workbenchCloneAnalyzeSoon)
  }

  const generate = () => {
    if (!analyzed) {
      setNotice(t.workbenchCloneNeedAnalyze)
      return
    }
    setNotice(t.workbenchImageSoon)
  }

  return (
    <WorkbenchPage mediaPool="imageModels"
      fill
      crumb={t.workbenchGroupImage}
      crumbCurrent={t.workbenchCloneCrumb}
      title={t.workbenchCloneTitle}
      actions={(
        <>
          <span className="workbench-capsule">{ref.length === 0 ? t.workbenchCloneWaitRef : t.workbenchImageRef}</span>
          <span className="workbench-capsule">{t.workbenchImageProduct} {products.length}/4</span>
        </>
      )}
    >
      <div className="workbench-split workbench-split--even">
        <div className="workbench-stack">
          <WorkbenchCard title={t.workbenchCloneStyle} hint={t.workbenchCloneStyleHint}>
            <CopyUploadField
              label={t.workbenchImageRef}
              required
              hint={t.workbenchImageFileHint}
              max={1}
              files={ref}
              onChange={(files) => {
                setRef(files)
                setAnalyzed(false)
              }}
              onNotice={setNotice}
            />
            <label className="workbench-field">
              <span>{t.workbenchClonePrompt}</span>
              <TextArea value={prompt} onChange={setPrompt} rows={5} placeholder={t.workbenchClonePromptHint} />
            </label>
            <WorkbenchCta notice={notice}>
              <Button onClick={analyze}>{t.workbenchCloneAnalyze}</Button>
            </WorkbenchCta>
          </WorkbenchCard>
          <WorkbenchCard title={t.workbenchImageConfig} hint={t.workbenchCloneConfigHint}>
            <CopyUploadField
              label={t.workbenchCloneProduct}
              optional
              max={4}
              files={products}
              onChange={setProducts}
              onNotice={setNotice}
            />
            <ImageSizeFields ratio={ratio} onRatio={setRatio} />
            <ImageCountField count={count} onCount={setCount} />
            <WorkbenchCta notice={notice}>
              <Button variant="primary" onClick={generate}>{t.workbenchCloneGenerate}</Button>
            </WorkbenchCta>
          </WorkbenchCard>
        </div>
        <WorkbenchCard fill title={t.workbenchImageResult} hint={t.workbenchCloneResultHint} extra={<span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>}>
          <WorkbenchEmpty icon={<Copy size={22} />} title={t.workbenchCloneEmpty}>
            {t.workbenchCloneEmptyHint}
          </WorkbenchEmpty>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
