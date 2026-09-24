import type { ShopPlatform } from '../../../api/tauri'
import shopeeLogo from './assets/shopee.svg'
import sheinLogo from './assets/shein.svg'
import tiktokLogo from './assets/tiktok.svg'
import mercadolibreLogo from './assets/mercadolibre.svg'
import douyinLogo from './assets/douyin.svg'
import kuaishouLogo from './assets/kuaishou.svg'
import wechatLogo from './assets/wechat.svg'
import taobaoLogo from './assets/taobao.svg'
import pinduoduoLogo from './assets/pinduoduo.png'
import './shopPlatformLogo.css'

const logos: Record<ShopPlatform, string> = {
  shopee: shopeeLogo,
  shein: sheinLogo,
  tiktok: tiktokLogo,
  mercadolibre: mercadolibreLogo,
  douyin: douyinLogo,
  kuaishou: kuaishouLogo,
  wechat: wechatLogo,
  taobao: taobaoLogo,
  pinduoduo: pinduoduoLogo,
}

export function ShopPlatformLogo({ platform }: { platform: ShopPlatform }) {
  return <span className={`workbench-platform-mark shop-platform-logo-frame shop-platform-logo-frame--${platform}`} aria-hidden="true">
    <img src={logos[platform]} alt="" />
  </span>
}
