import { useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField, revokeImages } from '../copy/CopyUploadField'
import { ImagePlatformLine, ImageSizeFields } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { sizeForImageRatio, type ImageRatioId } from './imageCatalog'

/**
 * 详情页：三步向导只做第一步上传。识别和模块生成还没接。
 */
export function DetailImagePage() {
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [roles, setRoles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [ratio, setRatio] = useState<ImageRatioId>('3:4')
  const [notice, setNotice] = useState('')

  const reset = () => {
    revokeImages(files)
    revokeImages(roles)
    setFiles([])
    setRoles([])
    setBrief('')
    setNotice('')
  }

  return (
    <WorkbenchPage
      fill
      crumb={t.workbenchGroupImage}
      crumbCurrent={t.workbenchDetailCrumb}
      title={t.workbenchDetailTitle}
      actions={(
        <>
          <span className="workbench-capsule">{t.workbenchImagePlatTb}</span>
          <span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>
        </>
      )}
    >
      <div className="workbench-steps">
        <span className="workbench-tab is-active">{t.workbenchDetailStep1}</span>
        <span className="workbench-tab">{t.workbenchDetailStep2}</span>
        <span className="workbench-tab">{t.workbenchDetailStep3}</span>
        <button type="button" className="workbench-tab" onClick={reset}>{t.workbenchDetailReset}</button>
      </div>
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchDetailStep1} hint={t.workbenchDetailUploadHint}>
          <CopyUploadField
            label={t.workbenchImageProduct}
            required
            max={10}
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
            <span>
              {t.workbenchDetailBrief}
              <span className="workbench-required" aria-hidden="true">*</span>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchDetailBriefHint} />
          </label>
          <ImagePlatformLine />
          <p className="workbench-page-sub">{t.workbenchDetailPlatformHint}</p>
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
          <p className="workbench-page-sub">{t.workbenchDetailNextHint}</p>
          <WorkbenchCta notice={notice}>
            <Button
              variant="primary"
              onClick={() => setNotice(files.length === 0 ? t.workbenchDetailNeedImage : t.workbenchDetailSoon)}
            >
              {t.workbenchDetailRecognize}
            </Button>
          </WorkbenchCta>
        </WorkbenchCard>
        <WorkbenchCard title={t.workbenchDetailPreview}>
          <div className="workbench-phone">
            <WorkbenchEmpty>{t.workbenchDetailEmptyHint}</WorkbenchEmpty>
          </div>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
