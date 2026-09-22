import { MediaTaskList } from '../MediaTaskList'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { readImages } from '../localMedia'
import { useState } from 'react'
import { Shuffle } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'

/**
 * 万物迁移：产品图进参考图场景。生成与历史使用统一媒体任务。
 */
export function MigratePage() {
  const generation = useMediaGeneration({ origin: workbenchOrigin('migrate') })
  const t = useT()
  const [products, setProducts] = useLocalImages()
  const [refs, setRefs] = useLocalImages()
  const [notice, setNotice] = useState('')


  return (
    <WorkbenchMediaModelSelect kind="imageModels" render={(modelControl, provider, model) => {
      const generate = async () => {
          if (generation.busy) return
          if (!products.length || !refs.length) { setNotice(t.workbenchImageNeedBoth); return }
          if (!provider || !model) { setNotice(t.workbenchMainNeedModel); return }
          if ([...products, ...refs].length > 4) { setNotice(t.workbenchImageTooManyRefs); return }
          setNotice('')
          await generation.submit(async () => ({ providerId: provider.id, model, kind: 'image',
            prompt: `Move the product in the first ${products.length} images into the scene of the final reference image. Preserve product identity and match scene lighting and perspective.`, images: await readImages([...products, ...refs]),
            options: { aspect_ratio: '1:1', size: '2K', n: 1 }, origin: workbenchOrigin('migrate') }))
      }
      return <ImageStudio
      modelControl={modelControl}
      results={generation.tasks.length || generation.loading ? <MediaTaskList bare generation={generation} alt={t.workbenchImageResult} /> : undefined}
      ctaDisabled={generation.busy}
      crumbCurrent={t.workbenchMigrateCrumb}
      title={t.workbenchMigrateTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t.workbenchImageProduct} {products.length}/4</span>
          <span className="workbench-capsule">{refs.length === 0 ? t.workbenchMigrateWaitRef : t.workbenchImageRef}</span>
        </>
      )}
      configTitle={t.workbenchImageConfig}
      configHint={t.workbenchMigrateHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchMigrateProduct}
            required
            max={4}
            files={products}
            onChange={setProducts}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchImageRef}
            required
            hint={t.workbenchImageFileHint}
            max={1}
            files={refs}
            onChange={setRefs}
            onNotice={setNotice}
          />
          <p className="workbench-page-sub">{t.workbenchMigrateGuideProduct}</p>
          <p className="workbench-page-sub">{t.workbenchMigrateGuideRef}</p>
        </>
      )}
      resultTitle={t.workbenchImagePreview}
      resultHint={t.workbenchMigrateResultHint}
      emptyIcon={<Shuffle size={22} />}
      emptyTitle={t.workbenchMigrateEmpty}
      emptyHint={t.workbenchMigrateEmptyHint}
      notice={notice || generation.error}
      cta={t.workbenchMigrateGenerate.replace('{n}', String(products.length))}
      onGenerate={() => void generate()}
    />
    }} />
  )
}
