import { afterEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { api } from '../../api/tauri'
import VideoStudio from './VideoStudio'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    videoStudioBootstrap: vi.fn(async () => ({
      tasks: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.14', comfy: false, node: true, ffmpeg: true },
      templates: [{ id: 'chat-template', name: '聊天创建的参考模板', kind: 'reference', script: '真实参考镜头' }],
    })),
    videoStudioImage: vi.fn(async () => ''),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
let dropHandler:
  | ((event: { payload: { type: string; paths?: string[] } }) => void)
  | undefined
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (fn: (event: { payload: { type: string; paths?: string[] } }) => void) => {
      dropHandler = fn
      return Promise.resolve(() => {
        if (dropHandler === fn) dropHandler = undefined
      })
    },
  }),
}))
vi.mock('../api', () => ({
  chatApi: {
    getAssistants: vi.fn().mockResolvedValue([]),
    optimizePrompt: vi.fn(),
  },
}))

describe('shared video workspace navigation', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('loads chat templates and starts a draft without silently choosing a generation route', async () => {
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '模板库 1' })
    fireEvent.click(screen.getByRole('button', { name: '模板库 1' }))
    await screen.findByRole('heading', { name: '聊天创建的参考模板' })
    fireEvent.click(screen.getByRole('button', { name: '使用模板' }))
    expect(await screen.findByText('已选模板：聊天创建的参考模板')).toBeTruthy()
    expect(screen.getByRole('button', { name: '生成路线' }).textContent).toContain('请选择')
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findByRole('heading', { name: '运行环境' })).toBeTruthy()
  })

  it('does not toast bootstrap failures when opening the page', async () => {
    vi.mocked(api.videoStudioBootstrap).mockRejectedValueOnce(
      '视频操作失败，请检查配置、依赖和网络。远程任务可通过任务编号继续查询。',
    )
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '模板库 1' })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('shows a floating error toast for a user action, then auto-dismisses', async () => {
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '模板库 1' })
    vi.mocked(api.videoStudioBootstrap).mockRejectedValueOnce(
      '视频操作失败，请检查配置、依赖和网络。远程任务可通过任务编号继续查询。',
    )
    vi.useFakeTimers()
    fireEvent.click(screen.getByRole('button', { name: '模板库 1' }))
    await act(async () => {
      await Promise.resolve()
    })
    const toast = screen.getByRole('alert')
    expect(toast.className).toContain('vs-toast')
    expect(toast.textContent).toContain('视频操作失败')
    expect(toast.closest('.is-main')).toBeNull()
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000)
    })
    expect(screen.queryByRole('alert')).toBeNull()
  })
})

describe('video material drag-drop', () => {
  afterEach(() => {
    dropHandler = undefined
  })

  it('imports dropped images into the creation drop zone', async () => {
    render(<VideoStudio />)
    const zone = await screen.findByLabelText('商品素材投放区')
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(zone).toHaveClass('is-drop-active'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\goods\\front.png'] } })
    expect(await screen.findByText('front.png')).toBeTruthy()
    expect(screen.getByLabelText('商品素材投放区')).toHaveClass('is-upload-area--filled')
    expect(screen.getByRole('button', { name: '继续添加参考图' })).toBeTruthy()
  })

  it('rejects a video drop on the creation page and accepts it in analysis', async () => {
    render(<VideoStudio />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\clips\\demo.mp4'] } })
    expect(await screen.findByRole('alert')).toHaveTextContent('请拖入商品图片')
    fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
    const zone = await screen.findByLabelText('参考视频投放区')
    expect(screen.getByText('把参考视频拖到这里')).toBeTruthy()
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(zone).toHaveClass('is-drop-active'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\clips\\demo.mp4'] } })
    expect(await screen.findByText('demo.mp4')).toBeTruthy()
    expect(screen.getByLabelText('参考视频投放区')).toHaveClass('is-upload-area--filled')
    expect(screen.getByPlaceholderText('粘贴视频链接，或把视频拖到这里')).toHaveValue(
      'C:\\clips\\demo.mp4',
    )
    expect(screen.getByRole('button', { name: '更换本地视频' })).toBeTruthy()
  })
})
