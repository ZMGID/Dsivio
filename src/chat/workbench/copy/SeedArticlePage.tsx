import { useEffect, useRef, useState } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import { FileText } from 'lucide-react'
import { api } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchCta, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { readImages, revokeImages, type LocalImage } from '../localMedia'
import { useAiTask } from '../useAiTask'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { readAssetText } from '../content/assetFiles'
import { CopyUploadField } from './CopyUploadField'
import { articleTitle, buildArticleRequest } from './articleRequest'
import {
  ARTICLE_LENGTHS,
  ARTICLE_PLATFORMS,
  ARTICLE_TYPES,
  type ArticleLengthId,
  type ArticlePlatformId,
  type ArticleTypeId,
} from './copyCatalog'

const ORIGIN = workbenchOrigin('articles')

type SavedArticle = { id: string; path: string; text: string }

/**
 * 种草文章：一步生成。产品信息、图片、平台、类型、篇幅组装成一次 `once` 请求，
 * 正文可改，保存进 `workbench/articles` 文案记录。
 */
export function SeedArticlePage() {
  const t = useT()
  const ai = useAiTask()
  const history = useMediaGeneration({ origin: ORIGIN })
  const [files, setFiles] = useState<LocalImage[]>([])
  const [brief, setBrief] = useState('')
  const [platform, setPlatform] = useState<ArticlePlatformId>('wechat')
  const [type, setType] = useState<ArticleTypeId>('seed')
  const [length, setLength] = useState<ArticleLengthId>('standard')
  const [markdown, setMarkdown] = useState('')
  const [saved, setSaved] = useState<SavedArticle | null>(null)
  const [notice, setNotice] = useState('')
  const [working, setWorking] = useState(false)
  const filesRef = useRef(files)
  filesRef.current = files
  const alive = useRef(true)
  const generation = useRef(0)
  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
      revokeImages(filesRef.current)
    }
  }, [])

  const platformMeta = ARTICLE_PLATFORMS.find((item) => item.id === platform) ?? ARTICLE_PLATFORMS[0]
  const typeMeta = ARTICLE_TYPES.find((item) => item.id === type) ?? ARTICLE_TYPES[0]
  const lengthMeta = ARTICLE_LENGTHS.find((item) => item.id === length) ?? ARTICLE_LENGTHS[1]
  const articles = history.tasks.filter((task) => task.kind === 'text' && task.status === 'succeeded' && task.outputs.length > 0)
  const alert = notice || ai.error || ''

  async function persist(text: string, prompt: string, generationId: number): Promise<SavedArticle | null> {
    const task = await api.recordMediaOutput({
      origin: ORIGIN,
      title: articleTitle(text, prompt),
      text,
      prompt: prompt.trim() || null,
    })
    if (!alive.current || generation.current !== generationId) return null
    const path = task.outputs[0]?.path
    if (!path) throw new Error(t.workbenchArticlesMissingFile)
    const next = { id: task.id, path, text }
    setSaved(next)
    setNotice('')
    history.refresh()
    return next
  }

  async function generate() {
    if (working || ai.busy) return
    if (!brief.trim() && files.length === 0) {
      setNotice(t.workbenchArticlesNeedBrief)
      return
    }
    const generationId = ++generation.current
    setWorking(true)
    setNotice('')
    try {
      const images = await readImages(files)
      if (!alive.current || generation.current !== generationId) return
      const request = buildArticleRequest({ brief, platform, type, length, imageCount: images.length })
      const text = await ai.run({ mode: 'once', system: request.system, prompt: request.prompt, images })
      if (!alive.current || generation.current !== generationId || text == null) return
      if (!text.trim()) {
        setNotice(t.workbenchArticlesEmptyResult)
        return
      }
      setMarkdown(text)
      await persist(text, brief, generationId)
    } catch (err) {
      if (!alive.current || generation.current !== generationId) return
      setNotice(err instanceof Error ? err.message : String(err))
    } finally {
      if (alive.current && generation.current === generationId) setWorking(false)
    }
  }

  async function saveCurrent() {
    if (working || !markdown.trim()) return
    const generationId = ++generation.current
    setWorking(true)
    setNotice('')
    try {
      const stored = await persist(markdown, brief, generationId)
      if (!alive.current || generation.current !== generationId) return
      if (!stored) return
    } catch (err) {
      if (!alive.current || generation.current !== generationId) return
      setNotice(err instanceof Error ? err.message : String(err))
    } finally {
      if (alive.current && generation.current === generationId) setWorking(false)
    }
  }

  async function ensureSaved(): Promise<SavedArticle | null> {
    if (saved && saved.text === markdown) return saved
    if (!markdown.trim()) {
      setNotice(t.workbenchArticlesEmptyResult)
      return null
    }
    const generationId = ++generation.current
    setWorking(true)
    try {
      return await persist(markdown, brief, generationId)
    } catch (err) {
      if (alive.current && generation.current === generationId) setNotice(err instanceof Error ? err.message : String(err))
      return null
    } finally {
      if (alive.current && generation.current === generationId) setWorking(false)
    }
  }

  async function openDocument() {
    setNotice('')
    const doc = await ensureSaved()
    if (!doc) return
    try {
      await api.openLocalFile(doc.path)
    } catch (err) {
      if (alive.current) setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  async function exportDocument() {
    setNotice('')
    const doc = await ensureSaved()
    if (!doc) return
    const destination = await save({
      defaultPath: `${articleTitle(doc.text, brief) || 'article'}.md`,
      filters: [{ name: 'Markdown', extensions: ['md'] }],
    })
    if (typeof destination !== 'string' || !alive.current) return
    try {
      await api.exportMediaOutput(doc.id, destination)
    } catch (err) {
      if (alive.current) setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  async function reopen(task: MediaTask) {
    const path = task.outputs[0]?.path
    if (!path) return
    setNotice('')
    try {
      const text = await readAssetText(path)
      if (!alive.current) return
      setMarkdown(text)
      setSaved({ id: task.id, path, text })
    } catch (err) {
      if (!alive.current) return
      setNotice(err instanceof Error ? err.message : String(err))
    }
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupCopy}
      crumbCurrent={t.workbenchArticlesCrumb}
      title={t.workbenchArticlesTitle}
      error={history.error || alert}
      onErrorDismiss={() => { setNotice(''); history.setError('') }}
      actions={(
        <>
          <span className="workbench-capsule">{t[platformMeta.name]}</span>
          <span className="workbench-capsule">{t[typeMeta.label]}</span>
          <span className="workbench-capsule">{t[lengthMeta.capsule]}</span>
        </>
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
          <WorkbenchCta>
            <Button variant="primary" disabled={working || ai.busy} onClick={() => void generate()}>
              {working || ai.busy ? t.workbenchArticlesGenerateBusy : t.workbenchArticlesGenerate}
            </Button>
          </WorkbenchCta>
        </WorkbenchCard>
      </div>

      <WorkbenchCard
        fill
        title={t.workbenchArticlesResult}
        hint={t.workbenchArticlesResultHint}
        extra={<Button size="sm" onClick={history.refresh}>{t.workbenchAssetsRefresh}</Button>}
      >
        <div className="workbench-field">
          <span>{t.workbenchArticlesHistory}</span>
          {history.loading && articles.length === 0 ? <p className="workbench-page-sub workbench-page-sub--flush">{t.workbenchAssetsLoading}</p> : null}
          {articles.length > 0 ? (
            <ul className="workbench-history-list">
              {articles.map((task) => {
                const title = task.result && typeof task.result === 'object' && task.result !== null && 'title' in task.result
                  ? String((task.result as { title?: unknown }).title ?? '')
                  : task.prompt
                return (
                  <li key={task.id}>
                    <button type="button" className="workbench-tab" onClick={() => void reopen(task)}>{title || task.id}</button>
                  </li>
                )
              })}
            </ul>
          ) : null}
        </div>
        {markdown ? (
          <>
            <TextArea value={markdown} onChange={setMarkdown} rows={16} placeholder={t.workbenchArticlesResultHint} />
            <div className="workbench-asset-actions">
              <Button size="sm" disabled={working || !markdown.trim()} onClick={() => void saveCurrent()}>{t.workbenchArticlesSave}</Button>
              <Button size="sm" disabled={working || !markdown.trim()} onClick={() => void openDocument()}>{t.workbenchArticlesOpen}</Button>
              <Button size="sm" disabled={working || !markdown.trim()} onClick={() => void exportDocument()}>{t.workbenchArticlesExport}</Button>
            </div>
          </>
        ) : !history.loading && articles.length === 0 ? (
          <WorkbenchEmpty icon={<FileText size={22} />} title={t.workbenchArticlesEmpty}>
            {t.workbenchArticlesEmptyHint}
          </WorkbenchEmpty>
        ) : null}
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
