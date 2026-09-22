import { MediaTaskList } from '../MediaTaskList'
import { useMediaGeneration, workbenchOrigin } from '../useMediaGeneration'
import { WorkbenchMediaModelSelect } from '../WorkbenchMediaModelSelect'
import { readImages } from '../localMedia'
import { useState } from 'react'
import { Pencil } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { TextArea } from '../../../settings/public/controls'
import { CopyUploadField } from '../copy/CopyUploadField'
import { revokeImages } from '../localMedia'
import { ImageSizeFields, ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'
import { sizeForImageRatio, type ImageRatioId } from './imageCatalog'

type EditTab = 'single' | 'multi'
type EditMode = 'fast' | 'rich'

/**
 * 图片编辑：上传 + 指令。框选改图还没接，多图只改上传张数。
 */
export function EditImagePage() {
  const generation = useMediaGeneration({ origin: workbenchOrigin('edit') })
  const t = useT()
  const [files, setFiles] = useLocalImages()
  const [brief, setBrief] = useState('')
  const [tab, setTab] = useState<EditTab>('single')
  const [mode, setMode] = useState<EditMode>('fast')
  const [ratio, setRatio] = useState<ImageRatioId>('1:1')
  const [notice, setNotice] = useState('')


  return (
    <WorkbenchMediaModelSelect kind="imageModels" render={(modelControl, provider, model) => {
      const generate = async () => {
          if (generation.busy) return
          if (!files.length) { setNotice(t.workbenchImageNeedProduct); return }
          if (!brief.trim()) { setNotice(t.workbenchEditNeedBrief); return }
          if (!provider || !model) { setNotice(t.workbenchMainNeedModel); return }
          if (files.length > 4) { setNotice(t.workbenchImageTooManyRefs); return }
          setNotice('')
          await generation.submit(async () => ({ providerId: provider.id, model, kind: 'image',
            prompt: `Edit the supplied image(s) according to this instruction: ${brief}. Preserve everything not explicitly requested to change.`, images: await readImages(files),
            options: { aspect_ratio: ratio, size: '2K', n: 1 }, origin: workbenchOrigin('edit') }))
      }
      return <ImageStudio
      modelControl={modelControl}
      results={generation.tasks.length || generation.loading ? <MediaTaskList bare generation={generation} alt={t.workbenchImageResult} /> : undefined}
      ctaDisabled={generation.busy}
      crumbCurrent={t.workbenchEditCrumb}
      title={t.workbenchEditTitle}
      capsules={(
        <>
          <span className="workbench-capsule">{tab === 'single' ? t.workbenchEditCurrent : t.workbenchEditMulti}</span>
          <span className="workbench-capsule">{t.workbenchEditResultCount} 0</span>
        </>
      )}
      tabs={(
        <div className="workbench-tabs">
          <button
            type="button"
            className={`workbench-tab${tab === 'single' ? ' is-active' : ''}`}
            onClick={() => {
              setTab('single')
              if (files.length > 1) {
                revokeImages(files.slice(1))
                setFiles(files.slice(0, 1))
              }
            }}
          >
            {t.workbenchEditSingle}
          </button>
          <button type="button" className={`workbench-tab${tab === 'multi' ? ' is-active' : ''}`} onClick={() => setTab('multi')}>
            {t.workbenchEditMulti}
          </button>
        </div>
      )}
      configTitle={t.workbenchEditUpload}
      configHint={t.workbenchEditUploadHint}
      config={(
        <>
          <CopyUploadField
            label={t.workbenchEditUpload}
            required
            hint={t.workbenchImageFileHint}
            max={tab === 'single' ? 1 : 4}
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <label className="workbench-field">
            <span>{t.workbenchEditBrief}</span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchEditBriefHint} />
          </label>
          <p className="workbench-page-sub">{t.workbenchEditBriefHint2}</p>
          <div className="workbench-field">
            <span>{t.workbenchEditMode}</span>
            <div className="workbench-tile-grid">
              <button type="button" className={`workbench-tile${mode === 'fast' ? ' is-active' : ''}`} onClick={() => setMode('fast')}>
                <span className="workbench-tile-name">{t.workbenchEditFast}</span>
                <span className="workbench-tile-desc">{t.workbenchEditFastDesc}</span>
              </button>
              <button type="button" className={`workbench-tile${mode === 'rich' ? ' is-active' : ''}`} onClick={() => setMode('rich')}>
                <span className="workbench-tile-name">{t.workbenchEditRich}</span>
                <span className="workbench-tile-desc">{t.workbenchEditRichDesc}</span>
              </button>
            </div>
          </div>
          <ImageSizeFields ratio={ratio} onRatio={setRatio} />
        </>
      )}
      resultTitle={t.workbenchEditResult}
      resultHint={t.workbenchEditResultHint}
      resultExtra={<span className="workbench-capsule">{sizeForImageRatio(ratio)}</span>}
      emptyIcon={<Pencil size={22} />}
      emptyTitle={t.workbenchEditEmpty}
      emptyHint={t.workbenchEditEmptyHint}
      notice={notice || generation.error}
      cta={t.workbenchEditGenerate}
      onGenerate={() => void generate()}
    />
    }} />
  )
}
