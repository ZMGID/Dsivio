import html from '../index.html?raw'
import { expect, it } from 'vitest'

const script = html.match(/<script>([\s\S]*?)<\/script>/)![1]
function boot(mode: string | null, systemDark: boolean, blocked = false) {
  const classes = new Set<string>()
  const run = new Function('navigator', 'document', 'localStorage', 'window', script)
  run(
    { userAgent: 'Mac' },
    { documentElement: { classList: { add: (name: string) => classes.add(name) } } },
    { getItem: () => { if (blocked) throw new Error('blocked'); return mode } },
    { matchMedia: () => ({ matches: systemDark }) },
  )
  return classes.has('dark')
}
it('启动画面沿用用户主题，避免先按系统主题绘制再闪色', () => {
  expect(boot('dark', false)).toBe(true)
  expect(boot('light', true)).toBe(false)
  expect(boot('system', true)).toBe(true)
  expect(boot('system', false)).toBe(false)
})
it('首次启动或存储不可用时按系统主题显示', () => {
  expect(boot(null, true)).toBe(true)
  expect(boot(null, false)).toBe(false)
  expect(boot(null, true, true)).toBe(true)
})
