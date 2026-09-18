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

describe('optimization draft safety', () => {
  it('keeps the draft when the backend rejects a truncated rewrite and allows retry', async () => {
    getAssistants.mockResolvedValue([])
    const message = '优化结果达到模型输出上限，内容未完成，已保留原文。'
    optimizePrompt.mockRejectedValueOnce(new Error(message))
    const change = vi.fn()
    const onError = vi.fn()
    render(<RequirementOptimize value="宣传这个" purpose="video_brief" onChange={change} onError={onError} />)
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() => expect(onError).toHaveBeenCalledWith(message))
    expect(change).not.toHaveBeenCalled()
    expect(screen.queryByRole('button', { name: '撤销优化' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '优化提示词' })).toBeEnabled()

    optimizePrompt.mockResolvedValueOnce('完整的视频要求')
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() => expect(change).toHaveBeenCalledWith('完整的视频要求'))
    expect(optimizePrompt).toHaveBeenLastCalledWith('宣传这个', null, {
      assistantId: null, purpose: 'video_brief', mediaPaths: [],
    })
  })

  it('does not overwrite edits made while the request is running', async () => {
    getAssistants.mockResolvedValue([])
    let finish!: (value: string) => void
    optimizePrompt.mockImplementationOnce(() => new Promise<string>(resolve => { finish = resolve }))
    const change = vi.fn()
    const props = { onChange: change, onError: vi.fn() }
    const view = render(<RequirementOptimize value="原描述" {...props} />)
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    view.rerender(<RequirementOptimize value="用户的新描述" {...props} />)
    finish('过期的结果')
    await waitFor(() => expect(screen.getByRole('button', { name: '优化提示词' })).toBeEnabled())
    expect(change).not.toHaveBeenCalled()
  })
  it('supports another optimization and a separate undo', async () => {
    getAssistants.mockResolvedValue([])
    optimizePrompt.mockResolvedValueOnce('优化结果')
    const change = vi.fn()
    const props = { onChange: change, onError: vi.fn() }
    const view = render(<RequirementOptimize value="原描述" {...props} />)
    fireEvent.click(screen.getByRole('button', { name: '优化提示词' }))
    await waitFor(() => expect(change).toHaveBeenCalledWith('优化结果'))
    view.rerender(<RequirementOptimize value="优化结果" {...props} />)
    expect(screen.getByRole('button', { name: '优化提示词' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '撤销优化' }))
    expect(change).toHaveBeenLastCalledWith('原描述')
  })
})
