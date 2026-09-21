import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { useChatRouteActive } from './chatRouteVisibility'
import { ChatRouteKeepAlive } from './ChatRouteKeepAlive'

describe('ChatRouteKeepAlive', () => {
  it('切换设置页后复用原聊天 DOM 实例', () => {
    const { rerender } = render(
      <ChatRouteKeepAlive activeKey="conversation">
        <main data-testid="chat-pane">chat</main>
      </ChatRouteKeepAlive>,
    )
    const firstPane = document.querySelector('[data-testid="chat-pane"]')
    expect(firstPane).not.toBeNull()

    rerender(
      <ChatRouteKeepAlive activeKey="settings">
        <section data-testid="settings-pane">settings</section>
      </ChatRouteKeepAlive>,
    )
    expect(firstPane).toBeInTheDocument()
    expect((firstPane?.parentElement as HTMLElement).hidden).toBe(true)
    expect(firstPane?.parentElement?.hasAttribute('inert')).toBe(true)

    rerender(
      <ChatRouteKeepAlive activeKey="conversation">
        <main data-testid="chat-pane">chat updated</main>
      </ChatRouteKeepAlive>,
    )
    expect(document.querySelector('[data-testid="chat-pane"]')).toBe(firstPane)
    expect((firstPane?.parentElement as HTMLElement).hidden).toBe(false)
    expect(firstPane).toHaveTextContent('chat updated')
  })
})

function MediaPane() {
  const active = useChatRouteActive()
  return <section data-testid="media-pane">{active ? 'active' : 'background'}</section>
}
it('preserves media state while hiding and deactivating a background page', () => {
 const { rerender, getByTestId } = render(<ChatRouteKeepAlive activeKey="images"><MediaPane /></ChatRouteKeepAlive>)
 const pane = getByTestId('media-pane')
 rerender(<ChatRouteKeepAlive activeKey="settings"><div>settings</div></ChatRouteKeepAlive>)
 expect(pane).toHaveTextContent('background')
 expect(pane.parentElement).toHaveAttribute('inert')
 rerender(<ChatRouteKeepAlive activeKey="images"><MediaPane /></ChatRouteKeepAlive>)
 expect(getByTestId('media-pane')).toBe(pane)
 expect(pane).toHaveTextContent('active')
})
