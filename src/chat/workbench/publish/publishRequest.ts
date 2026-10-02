import type { Privacy, PublishRequest } from '../../../generated/publish'
import { PUBLISH_DESC_MAX, PUBLISH_TITLE_MAX } from './publishCatalog'

export type PublishForm = {
  accountIds: string[]
  videoPath: string
  title: string
  description: string
  privacy: Privacy
  tags: string
  mediaTaskId?: string
}

export function buildPublishRequest(form: PublishForm): PublishRequest {
  const title = form.title.trim().slice(0, PUBLISH_TITLE_MAX)
  const description = form.description.trim().slice(0, PUBLISH_DESC_MAX)
  const accountIds = [...new Set(form.accountIds.map((id) => id.trim()).filter(Boolean))]
  const tags = form.tags
    .split(/[,，]/)
    .map((tag) => tag.trim().replace(/^#+/, ''))
    .filter(Boolean)
    .slice(0, 30)
  const request: PublishRequest = {
    accountIds,
    videoPath: form.videoPath.trim(),
    title,
    description,
    privacy: form.privacy,
    tags,
  }
  const mediaTaskId = form.mediaTaskId?.trim()
  if (mediaTaskId) request.mediaTaskId = mediaTaskId
  return request
}
