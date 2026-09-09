import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
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
    expect((firstPane?.parentElement as HTMLElement).style.display).toBe('none')

    rerender(
      <ChatRouteKeepAlive activeKey="conversation">
        <main data-testid="chat-pane">chat updated</main>
      </ChatRouteKeepAlive>,
    )
    expect(document.querySelector('[data-testid="chat-pane"]')).toBe(firstPane)
    expect(firstPane).toHaveTextContent('chat updated')
  })
})

it('retains both media workspaces and marks hidden native input listeners inactive', async () => {
  const { useChatRouteActive } = await import('./chatRouteVisibility')
  function Workspace({ name }: { name: string }) {
    const active = useChatRouteActive()
    return <main data-testid={name}>{active ? 'active' : 'background'}</main>
  }
  const { rerender } = render(<ChatRouteKeepAlive activeKey="videos"><Workspace name="video" /></ChatRouteKeepAlive>)
  const video = document.querySelector('[data-testid="video"]')
  rerender(<ChatRouteKeepAlive activeKey="images"><Workspace name="image" /></ChatRouteKeepAlive>)
  const image = document.querySelector('[data-testid="image"]')
  expect(video).toHaveTextContent('background')
  expect(image).toHaveTextContent('active')
  rerender(<ChatRouteKeepAlive activeKey="conversation"><main>新聊天</main></ChatRouteKeepAlive>)
  expect(video).toHaveTextContent('background')
  expect(image).toHaveTextContent('background')
  rerender(<ChatRouteKeepAlive activeKey="videos"><Workspace name="video" /></ChatRouteKeepAlive>)
  expect(document.querySelector('[data-testid="video"]')).toBe(video)
  expect(video).toHaveTextContent('active')
  expect(document.querySelector('[data-testid="image"]')).toBe(image)
})
