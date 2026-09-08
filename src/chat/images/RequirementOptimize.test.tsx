import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { RequirementOptimize } from './RequirementOptimize'

const getAssistants = vi.fn()
const optimizePrompt = vi.fn()

vi.mock('../api', () => ({
  chatApi: {
    getAssistants: () => getAssistants(),
    optimizePrompt: (
      text: string,
      conversationId?: string | null,
      options?: { assistantId?: string | null; purpose?: string },
    ) => optimizePrompt(text, conversationId, options),
  },
}))

describe('RequirementOptimize', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getAssistants.mockResolvedValue([
      {
        id: 'visual',
        name: '电商视觉',
        systemPrompt: '强调构图',
        created_at: 1,
        updated_at: 1,
      },
    ])
    optimizePrompt.mockResolvedValue('优化后的出图要求')
  })

  it('rewrites the brief with the selected assistant', async () => {
    const onChange = vi.fn()
    render(
      <RequirementOptimize
        value="白底主图，保留 logo"
        onChange={onChange}
        onError={() => {}}
      />,
    )
    fireEvent.click(await screen.findByRole('button', { name: '选择助手' }))
    fireEvent.click(await screen.findByRole('option', { name: '电商视觉' }))
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() =>
      expect(optimizePrompt).toHaveBeenCalledWith('白底主图，保留 logo', null, {
        assistantId: 'visual',
        purpose: 'image_brief',
      }),
    )
    expect(onChange).toHaveBeenCalledWith('优化后的出图要求')
  })
})
