import type { I18n } from '../../../components/i18n'
import type { PublishStatus, StatKey, VideoStats } from '../../../generated/publish'

export const PUBLISH_TITLE_MAX = 2200
export const PUBLISH_DESC_MAX = 5000

export const PUBLISH_LOG_TABS: { id: 'all' | 'queued' | 'done'; label: keyof I18n }[] = [
  { id: 'all', label: 'workbenchPlogsTabAll' },
  { id: 'queued', label: 'workbenchPlogsTabQueued' },
  { id: 'done', label: 'workbenchPlogsTabDone' },
]

export const ACCOUNT_KPIS: { id: 'bound' | 'ok' | 'bad' | 'fans'; label: keyof I18n; kind: 'count' }[] = [
  { id: 'bound', label: 'workbenchVacctsKpiBound', kind: 'count' },
  { id: 'ok', label: 'workbenchVacctsKpiOk', kind: 'count' },
  { id: 'bad', label: 'workbenchVacctsKpiBad', kind: 'count' },
  { id: 'fans', label: 'workbenchVacctsKpiFans', kind: 'count' },
]

export const QUEUED_STATUSES: PublishStatus[] = ['uploading', 'processing', 'uncertain']

export const STAT_COLUMNS: { key: StatKey; label: keyof I18n }[] = [
  { key: 'views', label: 'workbenchPdataColViews' },
  { key: 'likes', label: 'workbenchPdataColLikes' },
  { key: 'comments', label: 'workbenchPdataColComments' },
  { key: 'shares', label: 'workbenchPdataColShares' },
]

const STATUS_LABELS: Record<PublishStatus, keyof I18n> = {
  uploading: 'workbenchPublishStatusUploading',
  processing: 'workbenchPublishStatusProcessing',
  published: 'workbenchPublishStatusPublished',
  rejected: 'workbenchPublishStatusRejected',
  failed: 'workbenchPublishStatusFailed',
  uncertain: 'workbenchPublishStatusUncertain',
}

export function publishStatusLabel(status: PublishStatus): keyof I18n {
  return STATUS_LABELS[status]
}

/** A missing or listed-unsupported key is not zero. A returned 0 stays 0. */
export function displayedStat(stats: VideoStats | undefined, key: StatKey): number | 'unsupported' | 'pending' {
  if (!stats) return 'pending'
  const value = stats.values[key]
  if (stats.unsupported.includes(key) || value === undefined) return 'unsupported'
  return value
}
