import { RouteActiveContext } from './chatRouteVisibility'
import { useRef, type ReactNode } from 'react'

interface ChatRouteKeepAliveProps {
  activeKey: string
  children: ReactNode
}
// Media work continues while another route is visible.

export function ChatRouteKeepAlive({ activeKey, children }: ChatRouteKeepAliveProps) {
  const cacheRef = useRef(new Map<string, ReactNode>())
  const keep = activeKey === 'conversation' || activeKey === 'settings' || activeKey === 'videos' || activeKey === 'images'
  if (keep) cacheRef.current.set(activeKey, children)

  return (
    <div className="contents">
      {[...cacheRef.current.entries()].map(([key, node]) => (
        <div
          key={key}
          style={{ display: key === activeKey ? 'contents' : 'none' }}
          aria-hidden={key === activeKey ? undefined : true}
        >
          <RouteActiveContext.Provider value={key === activeKey}>{node}</RouteActiveContext.Provider>
        </div>
      ))}
      {!keep && <div style={{ display: 'contents' }}>{children}</div>}
    </div>
  )
}
