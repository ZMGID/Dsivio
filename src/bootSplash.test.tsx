import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import html from '../index.html?raw'
import { BOOT_SPLASH_ID, BOOT_SPLASH_LEAVE_MS, BOOT_SPLASH_MIN_MS, dismissBootSplash } from './bootSplash'

describe('window boot splash', () => {
  const splashScript = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)][1][1]
  const boot = new Function('location', 'document', 'performance', splashScript)

  beforeEach(() => {
    document.body.innerHTML = html.match(/<body>([\s\S]*?)<\/body>/)![1]
  })
  afterEach(() => {
    document.body.innerHTML = ''
  })

  it('hides the splash before startup scripts and app styles load', () => {
    const style = document.createElement('style')
    style.textContent = html.match(/<style>([\s\S]*?)<\/style>/)![1]
    document.body.append(style)
    expect(getComputedStyle(document.getElementById(BOOT_SPLASH_ID)!).display).toBe('none')
  })

  it.each(['', '#lens', '#translate', '#chat/popout/example'])(
    'removes the Chat splash from window route %s', (hash) => {
      boot({ hash }, document, { now: () => 1000 })
      expect(document.getElementById(BOOT_SPLASH_ID)).toBeNull()
    },
  )

  it.each(['#chat', '#chat/example', '#chat/settings?tab=translate'])(
    'keeps the Chat splash visible until content is ready for %s', (hash) => {
      boot({ hash }, document, { now: () => 1000 })
      const el = document.getElementById(BOOT_SPLASH_ID)!
      expect(el.hidden).toBe(false)
      expect(el.dataset.shownAt).toBe('1000')
    },
  )
})

describe('dismissBootSplash', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    document.body.innerHTML = ''
  })
  afterEach(() => {
    vi.useRealTimers()
    document.body.innerHTML = ''
  })

  it('no-ops when the splash node is missing', () => {
    dismissBootSplash(0)
    expect(document.getElementById(BOOT_SPLASH_ID)).toBeNull()
  })

  it('holds the minimum time from shownAt then removes the node', () => {
    document.body.innerHTML = `<div id="${BOOT_SPLASH_ID}" data-shown-at="1000"></div>`
    dismissBootSplash(1200)
    const el = document.getElementById(BOOT_SPLASH_ID)!
    expect(el.dataset.done).toBeUndefined()
    vi.advanceTimersByTime(BOOT_SPLASH_MIN_MS - 201)
    expect(el.dataset.done).toBeUndefined()
    vi.advanceTimersByTime(1)
    expect(el.dataset.done).toBe('1')
    expect(el.classList.contains('is-leaving')).toBe(true)
    vi.advanceTimersByTime(BOOT_SPLASH_LEAVE_MS)
    expect(document.getElementById(BOOT_SPLASH_ID)).toBeNull()
  })
})
