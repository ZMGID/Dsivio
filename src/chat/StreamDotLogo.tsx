import { memo, type CSSProperties } from 'react'
import logoMask from './stream-dot-logo.svg?raw'

// Inline the static mask so the first loading frame never waits for an image request.
const LOGO_MASK = `url("data:image/svg+xml,${encodeURIComponent(logoMask)}")`

/** Static brand dots with one transform sweep, shared by boot and conversation loading. */
export const StreamDotLogo = memo(function StreamDotLogo({ size = 104 }: { size?: number }) {
  return (
    <span
      aria-hidden="true"
      className="kv-stream-logo kv-stream-dot-logo"
      style={{
        '--kv-stream-logo-size': `${size}px`,
        '--kv-stream-logo-mask': LOGO_MASK,
      } as CSSProperties}
    />
  )
})
