import { useState } from 'react'
import { Image } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { readImages } from '../localMedia'
import { MediaTaskList } from '../MediaTaskList'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { ImageCountField, ImagePlatformLine, ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import {
  IMAGE_STYLES,
  sizeForImageRatio,
  type ImageCount,
  type ImageRatioId,
  type ImageStyleId,
} from './imageCatalog'
import { buildMainImagePrompt } from './mainImagePrompt'

const ORIGIN = workbenchOrigin('main')
/** 后端一次生成最多带 4 张参考图。 */
const MAX_REFERENCES = 4

/**
 * 主图：产品图 + 风格 + 尺寸 → 一次生图请求（n = 张数）。
 * 模型来自工作台图片模型池；历史按本页来源列出，与当前选的模型无关。
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
  const generation = useMediaGeneration({ origin: ORIGIN })
  const styleMeta = IMAGE_STYLES.find((item) => item.id === style) ?? IMAGE_STYLES[0]

  return (
    <WorkbenchMediaModelSelect
      kind="imageModels"
      render={(modelControl, provider, model) => {
        const generate = async () => {
          if (generation.busy) return
          if (!provider || !model) { setNotice(t.workbenchMainNeedModel); return }
          if (files.length === 0) { setNotice(t.workbenchImageNeedProduct); return }
          if (!brief.trim()) { setNotice(t.workbenchMainNeedBrief); return }
          if (files.length + roles.length > MAX_REFERENCES) { setNotice(t.workbenchImageTooManyRefs); return }
          setNotice('')
          generation.hold(true)
          let images: string[]
          try {
            images = await readImages([...files, ...roles])
          } catch (failure) {
            generation.hold(false)
            generation.setError(String(failure))
            return
          }
          generation.hold(false)
          await generation.submit({
            providerId: provider.id,
            model,
            kind: 'image',
            prompt: buildMainImagePrompt({ brief, style, ratio }, t),
            images,
            options: { aspect_ratio: ratio, size: '2K', n: count },
            origin: ORIGIN,
          })
        }
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
            modelControl={modelControl}
            configTitle={t.workbenchImageConfig}
            configHint={t.workbenchMainHint}
            config={(
              <fieldset disabled={generation.busy} className="contents">
                <CopyUploadField
                  label={t.workbenchImageProduct}
                  required
                  max={MAX_REFERENCES}
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
              </fieldset>
            )}
            resultTitle={t.workbenchImageResult}
            resultExtra={<Button size="sm" disabled={generation.loading || generation.busy} onClick={generation.refresh}>{t.workbenchRefresh}</Button>}
            results={generation.tasks.length > 0 || generation.loading ? <MediaTaskList bare generation={generation} alt={t.workbenchMainTitle} /> : undefined}
            emptyIcon={<Image size={22} />}
            emptyTitle={t.workbenchMainEmpty}
            emptyHint={t.workbenchMainEmptyHint}
            notice={notice || generation.error}
            cta={generation.busy ? t.workbenchMainSubmitting : t.workbenchMainGenerate}
            ctaDisabled={generation.busy}
            onGenerate={() => void generate()}
          />
        )
      }}
    />
  )
}
