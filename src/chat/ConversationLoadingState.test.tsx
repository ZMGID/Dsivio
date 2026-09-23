import { act, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ConversationLoadingState } from './ConversationLoadingState'

function hasLogo(container: HTMLElement): boolean {
  return container.querySelector('.kv-stream-dot-logo') !== null
}

describe('ConversationLoadingState', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows the dot logo on its first render', () => {
    const { container } = render(<ConversationLoadingState />)
    expect(hasLogo(container)).toBe(true)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('removes the logo as soon as loading finishes', () => {
    const { container, unmount } = render(<ConversationLoadingState />)
    unmount()
    act(() => {
      vi.advanceTimersByTime(300)
    })
    expect(container.querySelector('.chat-conversation-loading')).toBeNull()
  })
})
