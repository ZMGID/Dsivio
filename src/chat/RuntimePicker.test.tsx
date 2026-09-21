import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { RuntimePicker } from './RuntimePicker'

describe('single runtime identity', () => {
  it.each(['builtin', 'chat'] as const)('shows Dsivio Agent without a mode menu for %s', (kind) => {
    render(<RuntimePicker agentRuntime={{ kind }} onRuntimeChange={vi.fn()} />)
    expect(screen.getByLabelText('Dsivio Agent')).toBeInTheDocument()
    expect(screen.queryByRole('button')).toBeNull()
    expect(screen.queryByRole('menu')).toBeNull()
  })
  it('keeps CLI history visibly distinct', () => {
    render(<RuntimePicker agentRuntime={{ kind: 'external', externalAgentId: 'codex' }} onRuntimeChange={vi.fn()} />)
    expect(screen.getByLabelText('codex · 历史记录')).toBeInTheDocument()
  })
})
