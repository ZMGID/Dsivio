import { useLayoutEffect, useRef, type ReactNode } from 'react'

/** Commit the natural height once; fade user-opened details without making
 * virtual row measurement and scroll compensation run on every animation frame. */
export function ChatDisclosureBody({
  open,
  animate = true,
  keepMounted = false,
  children,
}: {
  open: boolean
  animate?: boolean
  keepMounted?: boolean
  children: ReactNode | (() => ReactNode)
}) {
  const previousOpen = useRef(open)
  const boxRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const box = boxRef.current
    if (!box) return
    box.inert = !open
    const wasOpen = previousOpen.current
    previousOpen.current = open
    if (wasOpen || !open || !animate || typeof box.animate !== 'function'
      || window.matchMedia('(prefers-reduced-motion: reduce)').matches) return
    const animation = box.animate(
      [{ opacity: 0 }, { opacity: 1 }],
      { duration: 160, easing: 'ease-out' },
    )
    return () => animation.cancel()
  }, [animate, open])

  return (
    <div ref={boxRef} data-chat-disclosure-body aria-hidden={!open} style={open ? undefined : { height: 0, overflow: 'clip' }}>
      <div style={{ display: 'flow-root' }}>
        {(open || keepMounted) && (typeof children === 'function' ? children() : children)}
      </div>
    </div>
  )
}
