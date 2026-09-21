import type { I18n } from '../../../components/i18n'

export const PUBLISH_TITLE_MAX = 30
export const PUBLISH_DESC_MAX = 200

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
