import type { I18n } from '../../../components/i18n'

export const ASSET_TABS: { id: 'all' | 'image' | 'video'; label: keyof I18n }[] = [
  { id: 'all', label: 'workbenchAssetsTabAll' },
  { id: 'image', label: 'workbenchAssetsTabImage' },
  { id: 'video', label: 'workbenchAssetsTabVideo' },
]
