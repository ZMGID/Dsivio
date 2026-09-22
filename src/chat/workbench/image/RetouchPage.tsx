import { MediaTaskList } from '../MediaTaskList'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { readImages } from '../localMedia'
import { useState } from 'react'
import { Sparkles } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { IMAGE_RETOUCH, sizeForImageRatio, type ImageRatioId, type ImageRetouchId } from './imageCatalog'

/**
 * 产品精修：一张参考图 + 模板。生成与历史使用统一媒体任务。
 */
export function RetouchPage() {
  const generation = useMediaGeneration({ origin: workbenchOrigin('retouch') })
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [template, setTemplate] = useState<ImageRetouchId>('white')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [notice, setNotice] = useState('')
  const templateMeta = IMAGE_RETOUCH.find((item) => item.id === template) ?? IMAGE_RETOUCH[0]

  return (
    <WorkbenchMediaModelSelect kind="imageModels" render={(modelControl, provider, model) => {
      const generate = async () => {
          if (generation.busy) return
          if (!files.length) { setNotice(t.workbenchImageNeedProduct); return }
          if (!provider || !model) { setNotice(t.workbenchMainNeedModel); return }
          if (files.length > 4) { setNotice(t.workbenchImageTooManyRefs); return }
          setNotice('')
          await generation.submit(async () => ({ providerId: provider.id, model, kind: 'image',
            prompt: `Retouch the supplied product image: ${t[templateMeta.name]}. ${t[templateMeta.desc]}. Preserve the product geometry, branding and materials.`, images: await readImages(files),
            options: { aspect_ratio: ratio, size: '2K', n: 1 }, origin: workbenchOrigin('retouch') }))
      }
      return <ImageStudio
      modelControl={modelControl}
      results={generation.tasks.length || generation.loading ? <MediaTaskList bare generation={generation} alt={t.workbenchImageResult} /> : undefined}
      ctaDisabled={generation.busy}
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
      notice={notice || generation.error}
      cta={t.workbenchRetouchGenerate}
      onGenerate={() => void generate()}
    />
    }} />
  )
}
