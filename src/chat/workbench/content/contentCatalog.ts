import type { I18n } from '../../../components/i18n'
import type { AssetKind } from './assetLibrary'

export const ASSET_TABS: { id: 'all' | AssetKind; label: keyof I18n }[] = [
  { id: 'all', label: 'workbenchAssetsTabAll' },
  { id: 'image', label: 'workbenchAssetsTabImage' },
  { id: 'video', label: 'workbenchAssetsTabVideo' },
  { id: 'edit', label: 'workbenchAssetsTabEdit' },
  { id: 'speech', label: 'workbenchAssetsTabSpeech' },
  { id: 'transcribe', label: 'workbenchAssetsTabTranscribe' },
  { id: 'text', label: 'workbenchAssetsTabText' },
]
