export const BOOT_SPLASH_ID = 'kv-boot-splash'
export const BOOT_SPLASH_MIN_MS = 500
export const BOOT_SPLASH_LEAVE_MS = 280

/** 首屏就绪后揭开：最短停 500ms，让点阵至少走完一小段。没有闪屏节点则空操作。 */
export function dismissBootSplash(now = performance.now()): void {
  const el = document.getElementById(BOOT_SPLASH_ID)
  if (!el || el.dataset.done === '1') return
  const shownAt = Number(el.dataset.shownAt || now)
  const wait = Math.max(0, BOOT_SPLASH_MIN_MS - (now - shownAt))
  const leave = () => {
    if (el.dataset.done === '1') return
    el.dataset.done = '1'
    el.classList.add('is-leaving')
    window.setTimeout(() => el.remove(), BOOT_SPLASH_LEAVE_MS)
  }
  window.setTimeout(leave, wait)
}
