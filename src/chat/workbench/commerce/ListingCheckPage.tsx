import { useMemo, useState } from 'react'
import { Button } from '../../../components/Button'
import { useT, type I18n } from '../../../components/i18n'
import { Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchPage } from '../WorkbenchPage'
import {
  checkListing,
  type ListingCheckItem,
  type ListingPlatformId,
} from './listingCheck'

function formatCheck(t: I18n, item: ListingCheckItem): string {
  const raw = t[`workbenchCheck_${item.id}` as keyof I18n]
  if (typeof raw !== 'string') return item.id
  return raw.replace(/\{(\w+)\}/g, (_, key: string) => String(item.params?.[key] ?? ''))
}

/**
 * 上架检查：粘贴标题和卖点，按平台规则扫一遍。
 * 不调模型、不写档案；字数和违禁词来自公开规范，见 listingCheck.ts。
 */
export function ListingCheckPage() {
  const t = useT()
  const [platform, setPlatform] = useState<ListingPlatformId>('douyin')
  const [title, setTitle] = useState('')
  const [sellingPoints, setSellingPoints] = useState('')
  const [description, setDescription] = useState('')
  const [ran, setRan] = useState(false)

  const result = useMemo(
    () => checkListing({ platform, title, sellingPoints, description }),
    [platform, title, sellingPoints, description],
  )

  const errors = result.items.filter((item) => item.level === 'error').length
  const warns = result.items.filter((item) => item.level === 'warn').length

  return (
    <WorkbenchPage
      title={t.workbenchNavCheck}
      subtitle={t.workbenchCheckSubtitle}
      actions={(
        <Select
          value={platform}
          onChange={(value) => setPlatform(value as ListingPlatformId)}
          ariaLabel={t.workbenchCheckPlatform}
          options={[
            { value: 'douyin', label: t.workbenchPlatformDouyin },
            { value: 'kuaishou', label: t.workbenchPlatformKuaishou },
            { value: 'wechat', label: t.workbenchPlatformWechat },
          ]}
        />
      )}
    >
      <div className="workbench-split">
        <WorkbenchCard title={t.workbenchCheckDraft}>
          <label className="workbench-field">
            <span>{t.workbenchCheckTitle}</span>
            <TextArea value={title} onChange={setTitle} rows={3} placeholder={t.workbenchCheckTitleHint} />
            <span className="workbench-page-sub">
              {result.titleLength}/{result.titleMax}
            </span>
          </label>
          <label className="workbench-field">
            <span>{t.workbenchCheckPoints}</span>
            <TextArea value={sellingPoints} onChange={setSellingPoints} rows={4} placeholder={t.workbenchCheckPointsHint} />
          </label>
          <label className="workbench-field">
            <span>{t.workbenchCheckDesc}</span>
            <TextArea value={description} onChange={setDescription} rows={5} placeholder={t.workbenchCheckDescHint} />
          </label>
          <Button variant="primary" onClick={() => setRan(true)}>{t.workbenchCheckRun}</Button>
        </WorkbenchCard>

        <WorkbenchCard
          title={t.workbenchCheckResult}
          extra={ran ? (
            <span className="workbench-page-sub">
              {t.workbenchCheckSummary.replace('{errors}', String(errors)).replace('{warns}', String(warns))}
            </span>
          ) : null}
        >
          {!ran ? (
            <p className="workbench-empty">{t.workbenchCheckIdle}</p>
          ) : (
            <ul className="workbench-check-list">
              {result.items.map((item) => (
                <li key={item.id} className={`workbench-check-item is-${item.level}`}>
                  <span className="workbench-check-mark">
                    {item.level === 'ok' ? t.workbenchCheckOk : item.level === 'warn' ? t.workbenchCheckWarn : t.workbenchCheckError}
                  </span>
                  <span>{formatCheck(t, item)}</span>
                </li>
              ))}
            </ul>
          )}
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
