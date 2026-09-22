import { useEffect, useRef, useState } from 'react'
import { FileText } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField } from './CopyUploadField'
import { revokeImages, type LocalImage } from '../localMedia'
import {
  ARTICLE_LENGTHS,
  ARTICLE_PLATFORMS,
  ARTICLE_TYPES,
  type ArticleLengthId,
  type ArticlePlatformId,
  type ArticleTypeId,
} from './copyCatalog'

/**
 * 种草文章：平台、类型、篇幅。开源版还没接写稿，结果区保持空。
 */
export function SeedArticlePage() {
  const t = useT()
  const [files, setFiles] = useState<LocalImage[]>([])
  const [brief, setBrief] = useState('')
  const [platform, setPlatform] = useState<ArticlePlatformId>('wechat')
  const [type, setType] = useState<ArticleTypeId>('seed')
  const [length, setLength] = useState<ArticleLengthId>('standard')
  const [notice, setNotice] = useState('')
  const filesRef = useRef(files)
  filesRef.current = files
  useEffect(() => () => revokeImages(filesRef.current), [])

  const platformMeta = ARTICLE_PLATFORMS.find((item) => item.id === platform) ?? ARTICLE_PLATFORMS[0]
  const typeMeta = ARTICLE_TYPES.find((item) => item.id === type) ?? ARTICLE_TYPES[0]
  const lengthMeta = ARTICLE_LENGTHS.find((item) => item.id === length) ?? ARTICLE_LENGTHS[1]

  const generate = () => {
    setNotice(files.length === 0 ? t.workbenchArticlesNeedImage : t.workbenchArticlesSoon)
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupCopy}
      crumbCurrent={t.workbenchArticlesCrumb}
      title={t.workbenchArticlesTitle}
      actions={(
        <div className="workbench-page-actions">
          <span className="workbench-capsule">{t[platformMeta.name]}</span>
          <span className="workbench-capsule">{t[typeMeta.label]}</span>
          <span className="workbench-capsule">{t[lengthMeta.capsule]}</span>
          <span className="workbench-capsule">{t.workbenchArticleAudience}</span>
        </div>
      )}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchCopyAssets} hint={t.workbenchCopyAssetsHint}>
          <CopyUploadField
            label={t.workbenchArticlesProduct}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span className="workbench-field-row">
              {t.workbenchArticlesBrief}
              <button type="button" className="workbench-tab" onClick={() => setBrief('')}>{t.workbenchCopyClear}</button>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={6} placeholder={t.workbenchArticlesBriefHint} />
          </label>
        </WorkbenchCard>

        <WorkbenchCard title={t.workbenchArticlesSettings} hint={t.workbenchArticlesSettingsHint}>
          <div className="workbench-field">
            <span>{t.workbenchArticlesPlatform}</span>
            <div className="workbench-tile-grid">
              {ARTICLE_PLATFORMS.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  className={`workbench-tile${platform === item.id ? ' is-active' : ''}`}
                  onClick={() => setPlatform(item.id)}
                >
                  <span className="workbench-tile-name">{t[item.name]}</span>
                  <span className="workbench-tile-desc">{t[item.desc]}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="workbench-pair">
            <div className="workbench-field">
              <span>{t.workbenchArticlesType}</span>
              <Select
                value={type}
                onChange={(value) => setType(value as ArticleTypeId)}
                ariaLabel={t.workbenchArticlesType}
                options={ARTICLE_TYPES.map((item) => ({ value: item.id, label: t[item.label] }))}
              />
            </div>
            <div className="workbench-field">
              <span>{t.workbenchArticlesLength}</span>
              <Select
                value={length}
                onChange={(value) => setLength(value as ArticleLengthId)}
                ariaLabel={t.workbenchArticlesLength}
                options={ARTICLE_LENGTHS.map((item) => ({ value: item.id, label: t[item.label] }))}
              />
            </div>
          </div>
          <div className="workbench-pair">
            <div className="workbench-field">
              <span>{t.workbenchArticlesAudience}</span>
              <Select
                value="mass"
                onChange={() => undefined}
                ariaLabel={t.workbenchArticlesAudience}
                options={[{ value: 'mass', label: t.workbenchArticleAudience }]}
              />
            </div>
            <div className="workbench-field">
              <span>{t.workbenchArticlesTone}</span>
              <Select
                value="real"
                onChange={() => undefined}
                ariaLabel={t.workbenchArticlesTone}
                options={[{ value: 'real', label: t.workbenchArticleTone }]}
              />
            </div>
          </div>
          <WorkbenchCta notice={notice}>
            <Button variant="primary" onClick={generate}>{t.workbenchArticlesGenerate}</Button>
          </WorkbenchCta>
        </WorkbenchCard>
      </div>

      <WorkbenchCard fill title={t.workbenchArticlesResult} hint={t.workbenchArticlesResultHint}>
        <WorkbenchEmpty icon={<FileText size={22} />} title={t.workbenchArticlesEmpty}>
          {t.workbenchArticlesEmptyHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
