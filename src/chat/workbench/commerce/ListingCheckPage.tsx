import { useMemo, useRef, useState } from 'react'
import { Button } from '../../../components/Button'
import { useT, type I18n } from '../../../components/i18n'
import { Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchPage } from '../WorkbenchPage'
import { useAiTask } from '../useAiTask'
import {
  buildListingAdviceRequest,
  checkListing,
  type ListingCheckItem,
  type ListingImageInput,
  type ListingPlatformId,
} from './listingCheck'
import { fileFromPath } from '../localMedia'
import { STORE_IMAGE_EXTENSIONS, useFileDrop } from '../useFileDrop'

function formatCheck(t: I18n, item: ListingCheckItem): string {
  const raw = t[`workbenchCheck_${item.id}` as keyof I18n]
  if (typeof raw !== 'string') return item.id
  return raw.replace(/\{(\w+)\}/g, (_, key: string) => String(item.params?.[key] ?? ''))
}

function readListingImages(files: File[]): Promise<ListingImageInput[]> {
  return Promise.all(files.map(async (file) => {
    const url = URL.createObjectURL(file)
    try {
      const dims = await new Promise<{ width?: number; height?: number }>((resolve) => {
        const img = new Image()
        img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight })
        img.onerror = () => resolve({})
        img.src = url
      })
      return { name: file.name, bytes: file.size, ...dims }
    } finally {
      URL.revokeObjectURL(url)
    }
  }))
}

const PLATFORM_ORDER: ListingPlatformId[] = [
  'douyin', 'kuaishou', 'wechat', 'taobao', 'pinduoduo',
  'shopee', 'shein', 'tiktok', 'mercadolibre',
]

/**
 * 上架检查：本地规则扫标题、文案和可选图片。
 * AI 建议只展示，不写回表单。
 */
export function ListingCheckPage() {
  const t = useT()
  const ai = useAiTask()
  const [platform, setPlatform] = useState<ListingPlatformId>('douyin')
  const [title, setTitle] = useState('')
  const [sellingPoints, setSellingPoints] = useState('')
  const [description, setDescription] = useState('')
  const [images, setImages] = useState<ListingImageInput[]>([])
  const [ran, setRan] = useState(false)
  const [suggestion, setSuggestion] = useState<string | null>(null)
  const suggestionGen = useRef(0)
  const fileRef = useRef<HTMLInputElement>(null)
  const imageZone = useRef<HTMLDivElement>(null)
  const [dropError, setDropError] = useState('')
  const imageOver = useFileDrop(imageZone, STORE_IMAGE_EXTENSIONS, (accepted, rejected) => {
    setDropError(rejected.length > 0 ? `${t.workbenchDropUnsupported}${STORE_IMAGE_EXTENSIONS.join(' / ')}` : '')
    if (accepted.length === 0) return
    void Promise.all(accepted.map(fileFromPath))
      .then(readListingImages)
      .then((next) => setImages((current) => [...current, ...next]))
      .catch((error) => setDropError(`${t.workbenchDropFailed}${error instanceof Error ? error.message : String(error)}`))
  })

  const result = useMemo(
    () => checkListing({ platform, title, sellingPoints, description, images }),
    [platform, title, sellingPoints, description, images],
  )

  const errors = result.items.filter((item) => item.level === 'error').length
  const warns = result.items.filter((item) => item.level === 'warn').length
  const labels: Record<ListingPlatformId, string> = {
    douyin: t.workbenchPlatformDouyin,
    kuaishou: t.workbenchPlatformKuaishou,
    wechat: t.workbenchPlatformWechat,
    taobao: t.workbenchPlatformTaobao,
    pinduoduo: t.workbenchPlatformPdd,
    shopee: t.workbenchPlatformShopee,
    shein: t.workbenchPlatformShein,
    tiktok: t.workbenchPlatformTiktok,
    mercadolibre: t.workbenchPlatformMeli,
  }

  function editTitle(next: string) {
    suggestionGen.current += 1
    setSuggestion(null)
    setTitle(next)
  }

  async function suggest() {
    const gen = ++suggestionGen.current
    setRan(true)
    const advice = buildListingAdviceRequest({ platform, title, sellingPoints, description, images }, result)
    const text = await ai.run({ mode: 'once', system: advice.system, prompt: advice.prompt, images: [] })
    if (gen !== suggestionGen.current || !text) return
    setSuggestion(text)
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupCommerce}
      title={t.workbenchNavCheck}
      subtitle={t.workbenchCheckSubtitle}
      actions={(
        <Select
          value={platform}
          onChange={(value) => setPlatform(value as ListingPlatformId)}
          ariaLabel={t.workbenchCheckPlatform}
          options={PLATFORM_ORDER.map((id) => ({ value: id, label: labels[id] }))}
        />
      )}
    >
      <div className="workbench-split">
        <WorkbenchCard title={t.workbenchCheckDraft}>
          <label className="workbench-field">
            <span>{t.workbenchCheckTitle}</span>
            <TextArea value={title} onChange={editTitle} rows={3} placeholder={t.workbenchCheckTitleHint} />
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
          <div ref={imageZone} className={`workbench-field workbench-drop-zone${imageOver ? ' is-drop-over' : ''}`}>
            <span>{t.workbenchCheckImages}</span>
            <input
              ref={fileRef}
              type="file"
              accept="image/png,image/jpeg,image/webp"
              multiple
              hidden
              aria-label={t.workbenchCheckAddImages}
              onChange={(event) => {
                const files = [...(event.target.files ?? [])]
                event.target.value = ''
                void readListingImages(files).then((next) => setImages((current) => [...current, ...next]))
              }}
            />
            <Button size="sm" onClick={() => fileRef.current?.click()}>{t.workbenchCheckAddImages}</Button>
            {images.length > 0 ? (
              <ul className="workbench-check-list">
                {images.map((image, index) => (
                  <li key={`${image.name ?? 'image'}-${index}`} className="workbench-check-item">
                    <span>{image.name || index + 1}</span>
                    <Button size="sm" onClick={() => setImages((current) => current.filter((_, item) => item !== index))}>{t.workbenchUploadRemove}</Button>
                  </li>
                ))}
              </ul>
            ) : null}
            {dropError ? <p className="workbench-inline-note" role="alert">{dropError}</p> : null}
          </div>
          <div className="workbench-cta-row">
            <Button variant="primary" onClick={() => setRan(true)}>{t.workbenchCheckRun}</Button>
            <Button onClick={() => void suggest()} disabled={ai.busy || !title.trim()}>{t.workbenchCheckAi}</Button>
          </div>
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
                <li key={`${item.id}-${JSON.stringify(item.params ?? {})}`} className={`workbench-check-item is-${item.level}`}>
                  <span className="workbench-check-mark">
                    {item.level === 'ok' ? t.workbenchCheckOk : item.level === 'warn' ? t.workbenchCheckWarn : t.workbenchCheckError}
                  </span>
                  <span>{formatCheck(t, item)}</span>
                </li>
              ))}
            </ul>
          )}
          {ai.busy ? <p className="workbench-page-sub">{t.workbenchCheckAiWorking}</p> : null}
          {ai.error ? <p className="workbench-inline-note">{ai.error}</p> : null}
          {suggestion ? (
            <p className="workbench-check-suggestion">{suggestion}</p>
          ) : (
            <p className="workbench-page-sub">{t.workbenchCheckAiIdle}</p>
          )}
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
