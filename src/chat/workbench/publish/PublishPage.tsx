import { useState } from 'react'
import { Share2 } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, TextArea } from '../../../settings/public/controls'
import { WorkbenchCard, WorkbenchEmpty, WorkbenchPage } from '../WorkbenchPage'
import { CopyUploadField } from '../copy/CopyUploadField'
import { useLocalImages } from '../image/useLocalImages'
import { VideoUploadField } from '../video/VideoUploadField'
import { useLocalVideo } from '../video/useLocalVideo'
import { PUBLISH_DESC_MAX, PUBLISH_TITLE_MAX } from './publishCatalog'

/**
 * 视频发布：本机视频 + 通用字段。平台分发还没接。
 */
export function PublishPage() {
  const t = useT()
  const [video, setVideo] = useLocalVideo()
  const [cover, setCover] = useLocalImages()
  const [title, setTitle] = useState('')
  const [desc, setDesc] = useState('')
  const [notice, setNotice] = useState('')

  const publish = () => {
    if (!video) {
      setNotice(t.workbenchVideoNeedFile)
      return
    }
    if (!title.trim()) {
      setNotice(t.workbenchPublishNeedTitle)
      return
    }
    setNotice(t.workbenchPublishSoon)
  }

  return (
    <WorkbenchPage
      fill
      crumb={t.workbenchGroupPublish}
      crumbCurrent={t.workbenchPublishCrumb}
      title={t.workbenchPublishTitle}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchPublishCommon} hint={t.workbenchPublishCommonHint}>
          <div className="workbench-card-scroll custom-scrollbar">
            <VideoUploadField
              label={t.workbenchPublishVideo}
              required
              hint={t.workbenchPublishVideoHint}
              file={video}
              onChange={setVideo}
              onNotice={setNotice}
            />
            <label className="workbench-field">
              <span className="workbench-field-row">
                <span>
                  {t.workbenchPublishHeadline}
                  <span className="workbench-required" aria-hidden="true">*</span>
                </span>
                <span className="workbench-page-sub workbench-page-sub--flush">{title.length}/{PUBLISH_TITLE_MAX}</span>
              </span>
              <Input
                value={title}
                onChange={(value) => setTitle(value.slice(0, PUBLISH_TITLE_MAX))}
                maxLength={PUBLISH_TITLE_MAX}
                placeholder={t.workbenchPublishHeadlineHint}
              />
            </label>
            <CopyUploadField
              label={t.workbenchPublishCover}
              optional
              hint={t.workbenchPublishCoverHint}
              max={1}
              files={cover}
              onChange={setCover}
              onNotice={setNotice}
            />
            <label className="workbench-field">
              <span className="workbench-field-row">
                <span>{t.workbenchPublishDesc}</span>
                <span className="workbench-page-sub workbench-page-sub--flush">{desc.length}/{PUBLISH_DESC_MAX}</span>
              </span>
              <TextArea
                value={desc}
                onChange={(value) => setDesc(value.slice(0, PUBLISH_DESC_MAX))}
                rows={5}
                placeholder={t.workbenchPublishDescHint}
              />
            </label>
            <div className="workbench-field">
              <span>{t.workbenchPublishWhen}</span>
              <div className="workbench-chip-row">
                <span className="workbench-capsule">{t.workbenchPublishNow}</span>
              </div>
            </div>
            {notice ? <p className="workbench-inline-note">{notice}</p> : null}
          </div>
          <div className="workbench-cta-row">
            <Button variant="primary" onClick={publish}>{t.workbenchPublishCta}</Button>
          </div>
        </WorkbenchCard>
        <WorkbenchCard fill title={t.workbenchPublishPlatforms} hint={t.workbenchPublishPlatformsHint}>
          <WorkbenchEmpty icon={<Share2 size={22} />} title={t.workbenchPublishPlatformsEmpty}>
            {t.workbenchPublishPlatformsEmptyHint}
          </WorkbenchEmpty>
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
