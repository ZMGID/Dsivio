import type { ShopPlatform } from '../../../api/tauri'

export const SHOP_PLATFORMS: readonly { id: ShopPlatform; name: string }[] = [
  { id: 'shopee', name: '虾皮 Shopee' },
  { id: 'shein', name: '希音 SHEIN' },
  { id: 'tiktok', name: 'TikTok Shop' },
  { id: 'mercadolibre', name: '美客多 Mercado Libre' },
]

// UI catalog only. A platform joins SHOP_PLATFORMS after its authorization backend is implemented.
export const UPCOMING_DOMESTIC_SHOP_PLATFORMS = [
  { id: 'douyin', name: '抖店', mark: '抖' },
  { id: 'kuaishou', name: '快手小店', mark: '快' },
  { id: 'wechat', name: '微信小店', mark: '微' },
  { id: 'taobao', name: '淘宝', mark: '淘' },
  { id: 'pinduoduo', name: '拼多多', mark: '拼' },
] as const
