import { useState } from 'react'
import { Shuffle } from 'lucide-react'
import { useT } from '../../../components/i18n'
import { CopyUploadField } from '../copy/CopyUploadField'
import { ImageStudio } from './ImageStudio'
import { useLocalImages } from './useLocalImages'

/**
 * 万物迁移：产品图进参考图场景。开源版还没接出图。
 */
export function MigratePage() {
  const t = useT()
  const [products, setProducts] = useLocalImages()
  const [refs, setRefs] = useLocalImages()
  const [notice, setNotice] = useState('')

  const generate = () => {
    if (products.length === 0 || refs.length === 0) {
      setNotice(t.workbenchImageNeedBoth)
      return
    }
    setNotice(t.workbenchImageSoon)
  }

  return (
    <ImageStudio
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
      notice={notice}
      cta={t.workbenchMigrateGenerate.replace('{n}', String(products.length))}
      onGenerate={generate}
    />
  )
}
