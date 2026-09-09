/**
 * @vitest-environment jsdom
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { LangContext } from '../settings/i18n'

vi.mock('./platform', async () => {
  const actual = await vi.importActual<typeof import('./platform')>('./platform')
  return {
    ...actual,
    usesNativeTitlebar: true,
  }
})

import { StudioPage } from './StudioPage'

describe('StudioPage', () => {
  it('does not reserve an empty titlebar row while the sidebar is open', () => {
    const { container } = render(
      <LangContext.Provider value="zh">
        <StudioPage
          sidebarCollapsed={false}
          onToggleSidebar={() => {}}
          onNewConversation={() => {}}
        >
          <div>studio</div>
        </StudioPage>
      </LangContext.Provider>,
    )

    const page = container.querySelector('.chat-studio-page')
    expect(page).toBeTruthy()
    expect(page?.className).not.toMatch(/\bpt-12\b/)
    expect(page?.classList.contains('chat-studio-page--collapsed')).toBe(false)
    expect(container.querySelector('.chat-studio-titlebar')).toBeNull()
    expect(screen.queryByRole('button', { name: '展开侧栏' })).toBeNull()
  })

  it('puts sidebar actions in an in-flow titlebar when the sidebar is collapsed', () => {
    const { container } = render(
      <LangContext.Provider value="zh">
        <StudioPage
          sidebarCollapsed
          onToggleSidebar={() => {}}
          onNewConversation={() => {}}
        >
          <div>studio</div>
        </StudioPage>
      </LangContext.Provider>,
    )

    const page = container.querySelector('.chat-studio-page')
    expect(page?.classList.contains('chat-studio-page--collapsed')).toBe(true)
    expect(page?.className).not.toMatch(/\bpt-12\b/)
    expect(container.querySelector('.chat-studio-titlebar')).toBeTruthy()
    expect(screen.getByRole('button', { name: '展开侧栏' })).toBeTruthy()
    expect(screen.getByRole('button', { name: '新建聊天' })).toBeTruthy()
  })
})
