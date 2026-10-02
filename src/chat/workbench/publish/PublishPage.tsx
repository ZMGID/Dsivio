import { useEffect, useRef, useState } from 'react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input, Select, TextArea } from '../../../settings/public/controls'
import { api, isTauriRuntime } from '../../../api/tauri'
import type { Privacy } from '../../../generated/publish'
import { WorkbenchCard, WorkbenchCta, WorkbenchPage } from '../WorkbenchPage'
import { PUBLISH_DESC_MAX, PUBLISH_TITLE_MAX, publishStatusLabel } from './publishCatalog'
import { buildPublishRequest } from './publishRequest'
import { usePublish } from './usePublish'
import { VIDEO_EXTENSIONS, useFileDrop } from '../useFileDrop'

const PRIVACY: Privacy[] = ['public', 'private', 'unlisted', 'friends']

/**
 * 视频发布：选已绑定账号和本机视频，组一条 PublishRequest，展示每个账号的结果。
 */
export function PublishPage() {
  const t = useT()
  const publish = usePublish()
  const { loadAccounts } = publish
  const [selected, setSelected] = useState<string[]>([])
  const [videoPath, setVideoPath] = useState('')
  const [title, setTitle] = useState('')
  const [description, setDescription] = useState('')
  const [privacy, setPrivacy] = useState<Privacy>('public')
  const [tags, setTags] = useState('')
  const [mediaTaskId, setMediaTaskId] = useState('')
  const [videos, setVideos] = useState<{ id: string; path: string; name: string }[]>([])
  const [notice, setNotice] = useState('')

  useEffect(() => {
    void loadAccounts()
    if (!isTauriRuntime()) return
    let active = true
    api.listMediaTasks({}).then((tasks) => {
      if (!active) return
      const rows = tasks.flatMap((task) => {
        if (task.kind !== 'video' || task.status !== 'succeeded') return []
        return task.outputs.filter((output) => output.path).map((output) => ({
          id: task.id,
          path: output.path,
          name: output.path.split(/[/\\]/).pop() || output.path,
        }))
      })
      setVideos(rows)
    }).catch(() => { /* 媒体任务列表失败时仍可手填路径 */ })
    return () => { active = false }
  }, [loadAccounts])

  const privacyLabel = (value: Privacy) => {
    if (value === 'public') return t.workbenchPublishPrivacyPublic
    if (value === 'private') return t.workbenchPublishPrivacyPrivate
    if (value === 'unlisted') return t.workbenchPublishPrivacyUnlisted
    return t.workbenchPublishPrivacyFriends
  }

  const toggle = (id: string) => {
    setSelected((previous) => previous.includes(id) ? previous.filter((item) => item !== id) : [...previous, id])
  }

  const pickFile = async () => {
    const dialog = await import('@tauri-apps/plugin-dialog')
    const picked = await dialog.open({ multiple: false, directory: false })
    if (typeof picked === 'string') {
      setVideoPath(picked)
      setMediaTaskId('')
    }
  }

  const videoZone = useRef<HTMLLabelElement>(null)
  const videoOver = useFileDrop(videoZone, VIDEO_EXTENSIONS, (accepted, rejected) => {
    if (accepted[0]) {
      setVideoPath(accepted[0])
      setMediaTaskId('')
    } else if (rejected.length > 0) {
      setNotice(`${t.workbenchDropUnsupported}${VIDEO_EXTENSIONS.join(' / ')}`)
    }
  })

  const submit = async () => {
    const request = buildPublishRequest({ accountIds: selected, videoPath, title, description, privacy, tags, mediaTaskId })
    if (request.accountIds.length === 0) {
      setNotice(t.workbenchPublishNeedAccount)
      return
    }
    if (!request.videoPath) {
      setNotice(t.workbenchPublishNeedVideo)
      return
    }
    if (!request.title) {
      setNotice(t.workbenchPublishNeedTitle)
      return
    }
    setNotice('')
    await publish.submit(request)
  }

  const shownNotice = notice || publish.error

  return (
    <WorkbenchPage
      fill
      crumb={t.workbenchGroupPublish}
      crumbCurrent={t.workbenchPublishCrumb}
      title={t.workbenchPublishTitle}
      error={shownNotice}
      onErrorDismiss={() => { setNotice(''); publish.setError('') }}
    >
      <div className="workbench-split workbench-split--even">
        <WorkbenchCard title={t.workbenchPublishCommon} hint={t.workbenchPublishCommonHint}>
          <label ref={videoZone} className={`workbench-field workbench-drop-zone${videoOver ? ' is-drop-over' : ''}`}>
            <span>{t.workbenchPublishVideoPath}</span>
            <Input
              aria-label={t.workbenchPublishVideoPath}
              value={videoPath}
              onChange={(value) => { setVideoPath(value); setMediaTaskId('') }}
              placeholder={t.workbenchPublishVideoHint}
            />
          </label>
          {isTauriRuntime() ? (
            <Button size="sm" onClick={() => { void pickFile() }}>{t.workbenchPublishPickFile}</Button>
          ) : null}
          {videos.length > 0 ? (
            <Select
              ariaLabel={t.workbenchPublishVideo}
              value={mediaTaskId}
              options={[{ value: '', label: t.workbenchPublishVideoHint }, ...videos.map((item) => ({ value: item.id, label: item.name }))]}
              onChange={(id) => {
                const match = videos.find((item) => item.id === id)
                setMediaTaskId(id)
                if (match) setVideoPath(match.path)
              }}
            />
          ) : null}
          <div className="workbench-field">
            <span>{t.workbenchPublishAccounts}</span>
            <div className="workbench-chip-row">
              {publish.accounts.map((account) => (
                <button
                  key={account.id}
                  type="button"
                  className="workbench-capsule"
                  aria-pressed={selected.includes(account.id)}
                  onClick={() => toggle(account.id)}
                >
                  {account.name}
                </button>
              ))}
            </div>
          </div>
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>
                {t.workbenchPublishHeadline}
                <span className="workbench-required" aria-hidden="true">*</span>
              </span>
              <span className="workbench-page-sub workbench-page-sub--flush">{title.length}/{PUBLISH_TITLE_MAX}</span>
            </span>
            <Input
              aria-label={t.workbenchPublishHeadline}
              value={title}
              onChange={(value) => setTitle(value.slice(0, PUBLISH_TITLE_MAX))}
              maxLength={PUBLISH_TITLE_MAX}
              placeholder={t.workbenchPublishHeadlineHint}
            />
          </label>
          <label className="workbench-field">
            <span className="workbench-field-row">
              <span>{t.workbenchPublishDesc}</span>
              <span className="workbench-page-sub workbench-page-sub--flush">{description.length}/{PUBLISH_DESC_MAX}</span>
            </span>
            <TextArea
              value={description}
              onChange={(value) => setDescription(value.slice(0, PUBLISH_DESC_MAX))}
              rows={5}
              placeholder={t.workbenchPublishDescHint}
            />
          </label>
          <div className="workbench-field">
            <span>{t.workbenchPublishPrivacy}</span>
            <Select
              ariaLabel={t.workbenchPublishPrivacy}
              value={privacy}
              options={PRIVACY.map((value) => ({ value, label: privacyLabel(value) }))}
              onChange={(value) => setPrivacy(value as Privacy)}
            />
          </div>
          <label className="workbench-field">
            <span>{t.workbenchPublishTags}</span>
            <Input
              aria-label={t.workbenchPublishTags}
              value={tags}
              onChange={setTags}
              placeholder={t.workbenchPublishTagsHint}
            />
          </label>
          <WorkbenchCta>
            <Button variant="primary" disabled={publish.pending} onClick={() => { void submit() }}>{t.workbenchPublishCta}</Button>
          </WorkbenchCta>
        </WorkbenchCard>
        <WorkbenchCard fill title={t.workbenchPublishResult}>
          {publish.result?.records.map((record) => (
            <p key={record.id}>
              {record.title} · {t[publishStatusLabel(record.status)]}
              {record.reason ? ` · ${record.reason}` : ''}
              {record.status === 'failed' || record.status === 'rejected' ? (
                <Button size="sm" onClick={() => { void publish.retry(record.id) }}>{t.workbenchPublishRetry}</Button>
              ) : null}
            </p>
          ))}
          {publish.result?.blocked.map((block) => (
            <p key={`${block.accountId}-${block.status}`}>{t.workbenchPublishBlocked} · {block.detail}</p>
          ))}
        </WorkbenchCard>
      </div>
    </WorkbenchPage>
  )
}
