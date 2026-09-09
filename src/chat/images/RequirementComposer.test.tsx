import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { RequirementComposer } from './RequirementComposer'

vi.mock('../api', () => ({
  chatApi: { getAssistants: vi.fn().mockResolvedValue([]), optimizePrompt: vi.fn() },
}))

describe('RequirementComposer', () => {
  it('keeps optimize outside the writing box', () => {
    render(
      <RequirementComposer
        label="制作要求"
        value=""
        placeholder="写清版式"
        onChange={() => {}}
        onError={() => {}}
      />,
    )
    const field = screen.getByLabelText('制作要求')
    expect(field).toHaveClass('kv-textarea', 'custom-scrollbar')
    expect(field.previousElementSibling).toHaveClass('is-req-bar')
    expect(field.previousElementSibling?.querySelector('.is-optimize')).toBeTruthy()
    expect(screen.getByRole('button', { name: '先写下图片要求或加载素材' })).toBeDisabled()
  })
})
