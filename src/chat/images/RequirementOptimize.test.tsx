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
        id: 'asst_builtin_ecom_visual',
        name: '电商生图',
        systemPrompt: '强调构图',
        installed: false,
        created_at: 1,
        updated_at: 1,
      },
      {
        id: 'asst_user_other',
        name: '写作助手',
        systemPrompt: '写文章',
        installed: true,
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
    fireEvent.click(await screen.findByRole('option', { name: '写作助手' }))
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() =>
      expect(optimizePrompt).toHaveBeenCalledWith('白底主图，保留 logo', null, {
        assistantId: 'asst_user_other',
        purpose: 'image_brief',
      }),
    )
    expect(onChange).toHaveBeenCalledWith('优化后的出图要求')
  })

  it('defaults to the preferred image expert even if it is not installed', async () => {
    const onChange = vi.fn()
    render(
      <RequirementOptimize
        value="白底主图，保留 logo"
        preferredAssistantId="asst_builtin_ecom_visual"
        purpose="image_brief"
        onChange={onChange}
        onError={() => {}}
      />,
    )
    expect(await screen.findByRole('button', { name: '电商生图' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() =>
      expect(optimizePrompt).toHaveBeenCalledWith('白底主图，保留 logo', null, {
        assistantId: 'asst_builtin_ecom_visual',
        purpose: 'image_brief',
      }),
    )
  })

  it('rewrites a video brief with the video expert', async () => {
    getAssistants.mockResolvedValue([
      {
        id: 'asst_builtin_video_prompt',
        name: '视频提示词',
        installed: false,
        created_at: 1,
        updated_at: 1,
      },
    ])
    optimizePrompt.mockResolvedValue('优化后的视频要求')
    const onChange = vi.fn()
    render(
      <RequirementOptimize
        value="背包展示一下"
        preferredAssistantId="asst_builtin_video_prompt"
        purpose="video_brief"
        onChange={onChange}
        onError={() => {}}
      />,
    )
    expect(await screen.findByRole('button', { name: '视频提示词' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() =>
      expect(optimizePrompt).toHaveBeenCalledWith('背包展示一下', null, {
        assistantId: 'asst_builtin_video_prompt',
        purpose: 'video_brief',
      }),
    )
    expect(onChange).toHaveBeenCalledWith('优化后的视频要求')
  })
})
