import { afterEach, describe, expect, it } from 'vitest'
import { installInputModality } from './inputModality'

describe('installInputModality', () => {
  let uninstall: () => void = () => {}
  afterEach(() => { uninstall(); delete document.documentElement.dataset.inputModality })

  it('starts as pointer so script focus after a click or native menu shows no ring', () => {
    uninstall = installInputModality()
    expect(document.documentElement.dataset.inputModality).toBe('pointer')
  })

  it('switches to keyboard only for navigation keys and back on pointer input', () => {
    uninstall = installInputModality()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }))
    expect(document.documentElement.dataset.inputModality).toBe('pointer')
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.documentElement.dataset.inputModality).toBe('keyboard')
    document.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    expect(document.documentElement.dataset.inputModality).toBe('pointer')
  })

  it('stops tracking once uninstalled', () => {
    installInputModality()()
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
    expect(document.documentElement.dataset.inputModality).toBe('pointer')
  })
})
