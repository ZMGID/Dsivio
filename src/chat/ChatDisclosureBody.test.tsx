import { render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ChatDisclosureBody } from './ChatDisclosureBody'

const animations: Array<{ cancel: ReturnType<typeof vi.fn> }> = []
const animate = vi.fn<(keyframes: Keyframe[], options: { duration: number; easing: string }) => Animation>(() => {
  const animation = { cancel: vi.fn() }
  animations.push(animation)
  return animation as unknown as Animation
})

beforeEach(() => {
  animations.length = 0
  animate.mockClear()
  vi.stubGlobal('matchMedia', () => ({ matches: false }))
  Object.defineProperty(HTMLElement.prototype, 'animate', { configurable: true, value: animate })
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect')
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  Reflect.deleteProperty(HTMLElement.prototype, 'animate')
})

describe('ChatDisclosureBody', () => {
  it('mounts details at natural height and fades without synchronously measuring layout', () => {
    const { rerender, container } = render(<ChatDisclosureBody open={false}>Details</ChatDisclosureBody>)
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
    rerender(<ChatDisclosureBody open>Details</ChatDisclosureBody>)
    expect(screen.getByText('Details')).toBeInTheDocument()
    expect(animate.mock.calls[0][0]).toEqual([{ opacity: 0 }, { opacity: 1 }])
    expect(HTMLElement.prototype.getBoundingClientRect).not.toHaveBeenCalled()
    expect((container.firstElementChild as HTMLElement).style.height).toBe('')
  })

  it('cancels a pending fade and releases heavy children immediately on close', () => {
    const { rerender, unmount } = render(<ChatDisclosureBody open={false}>Details</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open>Details</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open={false}>Details</ChatDisclosureBody>)
    expect(animations[0].cancel).toHaveBeenCalledOnce()
    expect(screen.queryByText('Details')).not.toBeInTheDocument()
    rerender(<ChatDisclosureBody open>Details</ChatDisclosureBody>)
    expect(screen.getByText('Details')).toBeInTheDocument()
    unmount()
    expect(animations[1].cancel).toHaveBeenCalledOnce()
  })

  it('lets late content grow naturally without restarting the fade or measuring geometry', () => {
    const { rerender, container } = render(<ChatDisclosureBody open={false}>Details</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open>Details</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open><div>Late image or code</div></ChatDisclosureBody>)
    expect(screen.getByText('Late image or code')).toBeInTheDocument()
    expect(animate).toHaveBeenCalledOnce()
    expect(HTMLElement.prototype.getBoundingClientRect).not.toHaveBeenCalled()
    expect((container.firstElementChild as HTMLElement).style.height).toBe('')
  })

  it('does not animate stream growth or automatic completion', () => {
    const { rerender } = render(<ChatDisclosureBody open animate={false}>First token</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open animate={false}>More tokens</ChatDisclosureBody>)
    rerender(<ChatDisclosureBody open={false} animate={false}>More tokens</ChatDisclosureBody>)
    expect(animate).not.toHaveBeenCalled()
    expect(screen.queryByText('More tokens')).not.toBeInTheDocument()
  })

  it('honors reduced motion and keeps hidden content inert when requested', () => {
    vi.stubGlobal('matchMedia', () => ({ matches: true }))
    const { rerender, container } = render(<ChatDisclosureBody open={false} keepMounted>Details</ChatDisclosureBody>)
    expect((container.firstElementChild as HTMLElement).inert).toBe(true)
    rerender(<ChatDisclosureBody open>Details</ChatDisclosureBody>)
    expect((container.firstElementChild as HTMLElement).inert).toBe(false)
    rerender(<ChatDisclosureBody open={false} keepMounted>Details</ChatDisclosureBody>)
    expect(animate).not.toHaveBeenCalled()
    expect(screen.getByText('Details')).toBeInTheDocument()
    expect((container.firstElementChild as HTMLElement).inert).toBe(true)
    expect(container.firstElementChild).toHaveAttribute('aria-hidden', 'true')
  })
})
