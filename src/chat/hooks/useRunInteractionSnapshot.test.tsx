import { act, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { PendingInteractionSlot } from '../PendingInteractionSlot'
import { createRunInteractionInbox } from '../runInteractionInbox'
import { useRunInteractionSnapshot } from './useRunInteractionSnapshot'

function prompt(conversationId: string) {
  return {
    conversationId, runId: `run-${conversationId}`, toolCallId: `ask-${conversationId}`,
    name: 'ask_user', source: 'native',
    prompt: { title: '确认范围', questions: [{
      id: 'scope', prompt: `${conversationId} 要处理哪些商品？`,
      options: [{ id: 'all', label: '全部商品' }, { id: 'selected', label: '指定商品' }],
    }] },
  }
}

function setup() {
  const inbox = createRunInteractionInbox({ confirmTool: vi.fn(), respondConsent: vi.fn() })
  function View({ id }: { id: string | null }) {
    const snapshot = useRunInteractionSnapshot(inbox, id)
    return <PendingInteractionSlot
      snapshot={snapshot}
      activeAgentRuntime={{ kind: 'builtin' }}
      onResolveToolConfirm={vi.fn()} onResolveSessionConsent={vi.fn()}
      onDismissUserPrompt={vi.fn()} onPersistApprovedSandbox={vi.fn()}
    />
  }
  return { inbox, View }
}

describe('useRunInteractionSnapshot', () => {
  it('shows a live question without requiring an imperative navigation activation', () => {
    const { inbox, View } = setup()
    render(<View id="existing" />)
    act(() => { inbox.observe({ kind: 'userPromptRequested', payload: prompt('existing') }) })
    expect(screen.getByRole('option', { name: /全部商品/ })).toBeInTheDocument()
    expect(screen.getByText('existing 要处理哪些商品？')).toBeInTheDocument()
  })

  it('follows the rendered conversation, retains background questions, and clears on leaving', () => {
    const { inbox, View } = setup()
    const view = render(<View id="a" />)
    act(() => { inbox.observe({ kind: 'userPromptRequested', payload: prompt('b') }) })
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    view.rerender(<View id="b" />)
    expect(screen.getByText('b 要处理哪些商品？')).toBeInTheDocument()
    view.rerender(<View id="a" />)
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    view.rerender(<View id="b" />)
    expect(screen.getByRole('option', { name: /全部商品/ })).toBeInTheDocument()
    view.rerender(<View id={null} />)
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })
})
