import { MediaTaskList } from '../MediaTaskList'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { readImages } from '../localMedia'
import { useState } from 'react'
import { Shirt } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'

type DressTab = 'dress' | 'pose'

function DressTabs({ tab, onTab }: { tab: DressTab; onTab: (tab: DressTab) => void }) {
  const t = useT()
  return (
    <div className="workbench-tabs">
      <button type="button" className={`workbench-tab${tab === 'dress' ? ' is-active' : ''}`} onClick={() => onTab('dress')}>
        {t.workbenchNavDress}
      </button>
      <button type="button" className={`workbench-tab${tab === 'pose' ? ' is-active' : ''}`} onClick={() => onTab('pose')}>
        {t.workbenchDressPose}
      </button>
    </div>
  )
}

/**
 * 一键换装：产品图 + 人物/场景参考。姿势裂变只留入口，不编造表单。
 */
export function DressPage() {
  const generation = useMediaGeneration({ origin: workbenchOrigin('dress') })
  const t = useT()
  const [product, setProduct] = useLocalImages()
  const [refs, setRefs] = useLocalImages()
  const [tab, setTab] = useState<DressTab>('dress')
  const [notice, setNotice] = useState('')

  const switchTab = (next: DressTab) => {
    setTab(next)
    setNotice('')
  }

  if (tab === 'pose') {
    return (
      <WorkbenchPage
        fill
        crumb={t.workbenchGroupImage}
        crumbCurrent={t.workbenchDressCrumb}
        title={t.workbenchDressTitle}
        actions={<span className="workbench-capsule">{t.workbenchDressPose}</span>}
      >
        <DressTabs tab={tab} onTab={switchTab} />
        <WorkbenchCard fill title={t.workbenchImagePreview} hint={t.workbenchDressPoseHint}>
          <WorkbenchEmpty icon={<Shirt size={22} />} title={t.workbenchDressPoseSoon} />
        </WorkbenchCard>
      </WorkbenchPage>
    )
  }

  return (
    <WorkbenchMediaModelSelect kind="imageModels" render={(modelControl, provider, model) => {
      const generate = async () => {
          if (generation.busy) return
          if (!product.length || !refs.length) { setNotice(t.workbenchImageNeedBoth); return }
          if (!provider || !model) { setNotice(t.workbenchMainNeedModel); return }
          if ([...product, ...refs].length > 4) { setNotice(t.workbenchImageTooManyRefs); return }
          setNotice('')
          await generation.submit(async () => ({ providerId: provider.id, model, kind: 'image',
            prompt: 'Dress the person in the reference images with the garment in the first product image. Preserve garment construction, pattern, color and the person identity.', images: await readImages([...product, ...refs]),
            options: { aspect_ratio: '1:1', size: '2K', n: 1 }, origin: workbenchOrigin('dress') }))
      }
      return <ImageStudio
      modelControl={modelControl}
      results={generation.tasks.length || generation.loading ? <MediaTaskList bare generation={generation} alt={t.workbenchImageResult} /> : undefined}
      ctaDisabled={generation.busy}
      crumbCurrent={t.workbenchDressCrumb}
      title={t.workbenchDressTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{t.workbenchDressCurrent}</span>
          <span className="workbench-capsule">{t.workbenchImageRef} {refs.length}/5</span>
        </>
      )}
      tabs={<DressTabs tab={tab} onTab={switchTab} />}
      configTitle={t.workbenchImageConfig}
      configHint={t.workbenchDressHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchDressProduct}
            required
            hint={t.workbenchImageFileHint}
            max={1}
            files={product}
            onChange={setProduct}
            onNotice={setNotice}
          />
          <CopyUploadField
            label={t.workbenchDressRef}
            required
            max={5}
            files={refs}
            onChange={setRefs}
            onNotice={setNotice}
          />
          <p className="workbench-page-sub">{t.workbenchDressGuideProduct}</p>
          <p className="workbench-page-sub">{t.workbenchDressGuideRef}</p>
        </>
      )}
      resultTitle={t.workbenchImagePreview}
      resultHint={t.workbenchDressResultHint}
      emptyIcon={<Shirt size={22} />}
      emptyTitle={t.workbenchDressEmpty}
      emptyHint={t.workbenchDressEmptyHint}
      notice={notice || generation.error}
      cta={t.workbenchDressGenerate.replace('{n}', String(refs.length))}
      onGenerate={() => void generate()}
    />
    }} />
  )
}
