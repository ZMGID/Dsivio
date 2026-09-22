import type { ReactNode } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import {
  IMAGE_COUNTS,
  IMAGE_RATIOS,
  sizeForImageRatio,
  type ImageCount,
  type ImageRatioId,
} from './imageCatalog'

export function ImageSizeFields({
  ratio,
  onRatio,
}: {
  ratio: ImageRatioId
  onRatio: (ratio: ImageRatioId) => void
}) {
  const t = useT()
  return (
    <div className="workbench-pair">
      <div className="workbench-field">
        <span>{t.workbenchImageRatio}</span>
        <Select
          value={ratio}
          onChange={(value) => onRatio(value as ImageRatioId)}
          ariaLabel={t.workbenchImageRatio}
          options={IMAGE_RATIOS.map((item) => ({ value: item.id, label: t[item.label] }))}
        />
      </div>
      <div className="workbench-field">
        <span>{t.workbenchImageSize}</span>
        <span className="workbench-readonly">{sizeForImageRatio(ratio)}</span>
      </div>
    </div>
  )
}

export function ImageCountField({
  count,
  onCount,
}: {
  count: ImageCount
  onCount: (count: ImageCount) => void
}) {
  const t = useT()
  return (
    <div className="workbench-field">
      <span>{t.workbenchImageCount}</span>
      <Select
        value={String(count)}
        onChange={(value) => onCount(Number(value) as ImageCount)}
        ariaLabel={t.workbenchImageCount}
        options={IMAGE_COUNTS.map((item) => ({ value: String(item), label: String(item) }))}
      />
    </div>
  )
}

/** 左配置 + 右预览。详情页向导和复刻两段式自己排。 */
export function ImageStudio({
  crumbCurrent,
  title,
  capsules,
  tabs,
  configTitle,
  configHint,
  config,
  resultTitle,
  resultHint,
  resultExtra,
  emptyIcon,
  emptyTitle,
  emptyHint,
  notice,
  cta,
  onGenerate,
  footer,
}: {
  crumbCurrent: string
  title: string
  capsules?: ReactNode
  tabs?: ReactNode
  configTitle: string
  configHint?: string
  config: ReactNode
  resultTitle: string
  resultHint?: string
  resultExtra?: ReactNode
  emptyIcon?: ReactNode
  emptyTitle: string
  emptyHint: string
  notice: string
  cta?: string
  onGenerate?: () => void
  footer?: ReactNode
}) {
  const t = useT()
  return (
    <WorkbenchPage mediaPool="imageModels" fill crumb={t.workbenchGroupImage} crumbCurrent={crumbCurrent} title={title} actions={capsules}>
      {tabs}
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={configTitle} hint={configHint}>
          {config}
          <WorkbenchCta notice={notice}>
            {footer ?? (cta && onGenerate ? <Button variant="primary" onClick={onGenerate}>{cta}</Button> : null)}
          </WorkbenchCta>
        </WorkbenchCard>
        <WorkbenchCard fill title={resultTitle} hint={resultHint} extra={resultExtra}>
          <WorkbenchEmpty icon={emptyIcon} title={emptyTitle}>{emptyHint}</WorkbenchEmpty>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}

export function ImagePlatformLine() {
  const t = useT()
  return (
    <div className="workbench-field">
      <span>{t.workbenchImagePlatform}</span>
      <div className="workbench-chip-row">
        <span className="workbench-capsule">{t.workbenchImagePlatTb}</span>
        <span className="workbench-capsule">{t.workbenchImageZh}</span>
      </div>
    </div>
  )
}
