import { useEffect, useRef, useState } from 'react'
import { Images } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Select, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField, revokeImages, type LocalImage } from './CopyUploadField'
import {
  POST_RATIOS,
  POST_TEMPLATES,
  sizeForRatio,
  type PostRatioId,
  type PostTemplateId,
} from './copyCatalog'

/**
 * 图文带货：平台模板 + 本机商品图。开源版还没接出图，结果区保持空。
 */
export function GraphicPostPage() {
  const t = useT()
  const [files, setFiles] = useState<LocalImage[]>([])
  const [roles, setRoles] = useState<LocalImage[]>([])
  const [brief, setBrief] = useState('')
  const [template, setTemplate] = useState<PostTemplateId>('xhs')
  const [ratio, setRatio] = useState<PostRatioId>('3:4')
  const [notice, setNotice] = useState('')
  const filesRef = useRef(files)
  const rolesRef = useRef(roles)
  filesRef.current = files
  rolesRef.current = roles
  useEffect(() => () => {
    revokeImages(filesRef.current)
    revokeImages(rolesRef.current)
  }, [])

  const templateMeta = POST_TEMPLATES.find((item) => item.id === template) ?? POST_TEMPLATES[0]
  const size = sizeForRatio(ratio)

  const generate = () => {
    setNotice(files.length === 0 ? t.workbenchPostsNeedImage : t.workbenchPostsSoon)
  }

  return (
    <WorkbenchPage
      crumb={t.workbenchGroupCopy}
      crumbCurrent={t.workbenchPostsCrumb}
      title={t.workbenchPostsTitle}
      actions={(
        <div className="workbench-page-actions">
          <span className="workbench-capsule">{t[templateMeta.name]}</span>
          <span className="workbench-capsule">{t.workbenchPostsCount}</span>
          <span className="workbench-capsule">{size}</span>
        </div>
      )}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchCopyAssets} hint={t.workbenchCopyAssetsHint}>
          <CopyUploadField
            label={t.workbenchPostsProduct}
            required
            files={files}
            onChange={setFiles}
            onNotice={setNotice}
          />
          <div className="workbench-role-block">
            <CopyUploadField
              label={t.workbenchPostsRole}
              optional
              hint=""
              max={3}
              files={roles}
              onChange={setRoles}
              onNotice={setNotice}
            />
          </div>
        </WorkbenchCard>

        <WorkbenchCard title={t.workbenchPostsSettings} hint={t.workbenchPostsSettingsHint}>
          <label className="workbench-field">
            <span className="workbench-field-row">
              {t.workbenchPostsBrief}
              <button type="button" className="workbench-tab" onClick={() => setBrief('')}>{t.workbenchCopyClear}</button>
            </span>
            <TextArea value={brief} onChange={setBrief} rows={5} placeholder={t.workbenchPostsBriefHint} />
          </label>
          <div className="workbench-field">
            <span>{t.workbenchPostsTemplate}</span>
            <div className="workbench-tile-grid">
              {POST_TEMPLATES.map((item) => (
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
            <p className="workbench-page-sub">{t.workbenchPostsTemplateHint.replace('{name}', t[templateMeta.name])}</p>
          </div>
          <div className="workbench-pair">
            <label className="workbench-field">
              <span>{t.workbenchPostsRatio}</span>
              <Select
                value={ratio}
                onChange={(value) => setRatio(value as PostRatioId)}
                ariaLabel={t.workbenchPostsRatio}
                options={POST_RATIOS.map((item) => ({ value: item.id, label: t[item.label] }))}
              />
            </label>
            <label className="workbench-field">
              <span>{t.workbenchPostsSize}</span>
              <span className="workbench-readonly">{size}</span>
            </label>
          </div>
          {notice ? <p className="workbench-inline-note">{notice}</p> : null}
          <div className="workbench-cta-row">
            <Button variant="primary" onClick={generate}>{t.workbenchPostsGenerate}</Button>
          </div>
        </WorkbenchCard>
      </div>

      <WorkbenchCard fill title={t.workbenchPostsResult} hint={t.workbenchPostsResultHint} extra={<span className="workbench-capsule">{size}</span>}>
        <WorkbenchEmpty icon={<Images size={22} />} title={t.workbenchPostsEmpty}>
          {t.workbenchPostsEmptyHint}
        </WorkbenchEmpty>
      </WorkbenchCard>
    </WorkbenchPage>
  )
}
