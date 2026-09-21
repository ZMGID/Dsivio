import type { I18n } from '../../../components/i18n'

export type DramaStyleId =
  | 'twist' | 'romance' | 'comedy' | 'mystery' | 'workplace'
  | 'family' | 'ceo' | 'rebirth' | 'schemer'

export const DRAMA_STYLES: { id: DramaStyleId; label: keyof I18n }[] = [
  { id: 'twist', label: 'workbenchDramaStyleTwist' },
  { id: 'romance', label: 'workbenchDramaStyleRomance' },
  { id: 'comedy', label: 'workbenchDramaStyleComedy' },
  { id: 'mystery', label: 'workbenchDramaStyleMystery' },
  { id: 'workplace', label: 'workbenchDramaStyleWork' },
  { id: 'family', label: 'workbenchDramaStyleFamily' },
  { id: 'ceo', label: 'workbenchDramaStyleCeo' },
  { id: 'rebirth', label: 'workbenchDramaStyleRebirth' },
  { id: 'schemer', label: 'workbenchDramaStyleSchemer' },
]

export const VIDEO_TASK_TABS: { id: 'all' | 'queued' | 'running' | 'done' | 'failed'; label: keyof I18n }[] = [
  { id: 'all', label: 'workbenchVideoTabAll' },
  { id: 'queued', label: 'workbenchVideoTabQueued' },
  { id: 'running', label: 'workbenchVideoTabRunning' },
  { id: 'done', label: 'workbenchVideoTabDone' },
  { id: 'failed', label: 'workbenchVideoTabFailed' },
]
