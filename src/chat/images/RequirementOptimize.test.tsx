import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  collectBriefImagePaths,
  collectStudioMediaPaths,
  RequirementOptimize,
} from './RequirementOptimize'

const getAssistants = vi.fn()
const optimizePrompt = vi.fn()

vi.mock('../api', () => ({
  chatApi: {
    getAssistants: () => getAssistants(),
    optimizePrompt: (
      text: string,
      conversationId?: string | null,
      options?: { assistantId?: string | null; purpose?: string; mediaPaths?: string[] },
    ) => optimizePrompt(text, conversationId, options),
  },
}))

describe('collectStudioMediaPaths', () => {
  it('dedupes and puts stills before videos', () => {
    expect(
      collectStudioMediaPaths([
        '  bag.png  ',
        'bag.png',
        'clip.mp4',
        'front.webp',
        '',
        null,
        'unbox.MOV',
      ]),
    ).toEqual(['bag.png', 'front.webp', 'clip.mp4', 'unbox.MOV'])
  })
})

describe('collectBriefImagePaths', () => {
  it('uses asset paths and ignores front/back ids', () => {
    expect(
      collectBriefImagePaths({
        products: [
          {
            assets: [
              { path: 'assets/front.jpg' },
              { path: 'assets/front.jpg' },
              { path: 'assets/detail.png' },
            ],
          },
        ],
        workflowInput: { sources: [{ path: 'assets/h1.jpg' }] },
      }),
    ).toEqual(['assets/front.jpg', 'assets/detail.png', 'assets/h1.jpg'])
  })
})

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
        mediaPaths: [],
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
        mediaPaths: [],
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
        mediaPaths: [],
      }),
    )
    expect(onChange).toHaveBeenCalledWith('优化后的视频要求')
  })

  it('sends loaded studio media and can run from images alone', async () => {
    getAssistants.mockResolvedValue([
      {
        id: 'asst_builtin_video_prompt',
        name: '视频提示词',
        installed: false,
        created_at: 1,
        updated_at: 1,
      },
    ])
    optimizePrompt.mockResolvedValue('核心痛点：容量不够用')
    const onChange = vi.fn()
    render(
      <RequirementOptimize
        value="  "
        preferredAssistantId="asst_builtin_video_prompt"
        purpose="video_brief"
        mediaPaths={['C:\\studio\\bag.png', 'clip.mp4', 'C:\\studio\\bag.png']}
        onChange={onChange}
        onError={() => {}}
      />,
    )
    expect(await screen.findByRole('button', { name: '视频提示词' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() =>
      expect(optimizePrompt).toHaveBeenCalledWith('  ', null, {
        assistantId: 'asst_builtin_video_prompt',
        purpose: 'video_brief',
        mediaPaths: ['C:\\studio\\bag.png', 'clip.mp4'],
      }),
    )
    expect(onChange).toHaveBeenCalledWith('核心痛点：容量不够用')
  })
})
