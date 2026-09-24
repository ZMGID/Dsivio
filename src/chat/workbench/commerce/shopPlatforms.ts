import type { ShopPlatform } from '../../../api/tauri'

export const SHOP_PLATFORMS: readonly { id: ShopPlatform; name: string }[] = [
  { id: 'shopee', name: '虾皮 Shopee' },
  { id: 'shein', name: '希音 SHEIN' },
  { id: 'tiktok', name: 'TikTok Shop' },
  { id: 'mercadolibre', name: '美客多 Mercado Libre' },
]

export const DOMESTIC_SHOP_PLATFORMS: readonly { id: ShopPlatform; name: string }[] = [
  { id: 'douyin', name: '抖店' },
  { id: 'kuaishou', name: '快手小店' },
  { id: 'wechat', name: '微信小店' },
  { id: 'taobao', name: '淘宝' },
  { id: 'pinduoduo', name: '拼多多' },
]

export const ALL_SHOP_PLATFORMS: readonly { id: ShopPlatform; name: string }[] = [
  ...SHOP_PLATFORMS,
  ...DOMESTIC_SHOP_PLATFORMS,
]

// 微信小店没有网页授权页：填完小店 AppID/Secret 后直接完成绑定。
export function isDirectBindPlatform(platform: ShopPlatform) {
  return platform === 'wechat'
}
