import type { I18n } from '../../../components/i18n'

export type ImageRatioId = '1:1' | '3:4'
export type ImageStyleId =
  | 'white' | 'life' | 'luxe' | 'tech' | 'promo' | 'fresh' | 'seed' | 'brand' | 'custom'
export type ImageCoverId = 'brand' | 'xhs' | 'short' | 'main' | 'event' | 'buyer'
export type ImageRetouchId = 'white' | 'portrait' | 'scene' | 'creative' | 'detail' | 'color'
export type ImageCount = 1 | 2 | 3 | 4

export const IMAGE_RATIOS: { id: ImageRatioId; label: keyof I18n }[] = [
  { id: '1:1', label: 'workbenchImageRatio11' },
  { id: '3:4', label: 'workbenchImageRatio34' },
]

export const IMAGE_COUNTS: ImageCount[] = [1, 2, 3, 4]

export const IMAGE_STYLES: { id: ImageStyleId; label: keyof I18n }[] = [
  { id: 'white', label: 'workbenchImageStyleWhite' },
  { id: 'life', label: 'workbenchImageStyleLife' },
  { id: 'luxe', label: 'workbenchImageStyleLuxe' },
  { id: 'tech', label: 'workbenchImageStyleTech' },
  { id: 'promo', label: 'workbenchImageStylePromo' },
  { id: 'fresh', label: 'workbenchImageStyleFresh' },
  { id: 'seed', label: 'workbenchImageStyleSeed' },
  { id: 'brand', label: 'workbenchImageStyleBrand' },
  { id: 'custom', label: 'workbenchImageStyleCustom' },
]

export const IMAGE_COVERS: { id: ImageCoverId; label: keyof I18n }[] = [
  { id: 'brand', label: 'workbenchCoverBrand' },
  { id: 'xhs', label: 'workbenchCoverXhs' },
  { id: 'short', label: 'workbenchCoverShort' },
  { id: 'main', label: 'workbenchCoverMain' },
  { id: 'event', label: 'workbenchCoverEvent' },
  { id: 'buyer', label: 'workbenchCoverBuyer' },
]

export const IMAGE_RETOUCH: { id: ImageRetouchId; name: keyof I18n; desc: keyof I18n }[] = [
  { id: 'white', name: 'workbenchRetouchWhite', desc: 'workbenchRetouchWhiteDesc' },
  { id: 'portrait', name: 'workbenchRetouchPortrait', desc: 'workbenchRetouchPortraitDesc' },
  { id: 'scene', name: 'workbenchRetouchScene', desc: 'workbenchRetouchSceneDesc' },
  { id: 'creative', name: 'workbenchRetouchCreative', desc: 'workbenchRetouchCreativeDesc' },
  { id: 'detail', name: 'workbenchRetouchDetail', desc: 'workbenchRetouchDetailDesc' },
  { id: 'color', name: 'workbenchRetouchColor', desc: 'workbenchRetouchColorDesc' },
]

/** 主图默认 1:1 是 2048×2048；详情页活页是 3:4 / 1920×2560。 */
export function sizeForImageRatio(ratio: ImageRatioId): string {
  return ratio === '3:4' ? '1920×2560' : '2048×2048'
}
