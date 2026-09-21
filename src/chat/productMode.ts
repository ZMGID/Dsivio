/**
 * 主窗口的两种形态。
 *
 * 形态只决定「左边那根导航长什么样、切过去落在哪一页」，不决定页面本身：图片 / 视频 /
 * 自动化 / 作品库这些中心页两种形态都能打开，路由词表仍由 routeContract.json 独占。
 * 因此形态不写进 hash，也不参与 Rust 的上次路由恢复 —— 恢复出来的页面在哪个形态下都成立。
 */
export type ProductMode = 'chat' | 'workbench'

const PRODUCT_MODE_KEY = 'kivio.chat.productMode'

export const PRODUCT_MODES: readonly ProductMode[] = ['chat', 'workbench']

function isProductMode(value: string | null): value is ProductMode {
  return value === 'chat' || value === 'workbench'
}

export function loadProductMode(): ProductMode {
  try {
    const raw = window.localStorage?.getItem(PRODUCT_MODE_KEY) ?? null
    return isProductMode(raw) ? raw : 'chat'
  } catch {
    return 'chat'
  }
}

export function saveProductMode(mode: ProductMode): void {
  try {
    window.localStorage?.setItem(PRODUCT_MODE_KEY, mode)
  } catch {
    // 受限环境下没有 localStorage；形态仍在本次会话内可用，只是不记住。
  }
}

/** 切换形态后要落到的页面。形态自己不是一条路由，进入形态等于进入它的首页。 */
export function productModeHomeHash(mode: ProductMode): string {
  return mode === 'workbench' ? '#chat/workbench' : '#chat'
}
