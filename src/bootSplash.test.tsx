import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { BOOT_SPLASH_ID, BOOT_SPLASH_LEAVE_MS, BOOT_SPLASH_MIN_MS, dismissBootSplash } from './bootSplash'

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
