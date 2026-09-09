import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { api } from '../../api/tauri'
import VideoStudio from './VideoStudio'
import { newVideoBrief, type VideoTask } from './types'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    studioTaskLibrary: vi.fn(async () => ({})),
    videoStudioBootstrap: vi.fn(async () => ({
      tasks: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.14', comfy: false, node: true, ffmpeg: true },
      templates: [{ id: 'chat-template', name: '聊天创建的参考模板', kind: 'reference', script: '真实参考镜头' }],
    })),
    videoStudioImage: vi.fn(async () => ''),
    videoStudioTask: vi.fn(),
    videoStudioPreview: vi.fn(async () => ''),
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
  it('opens older chat-created tasks from a dedicated searchable library while keeping the rail bounded', async () => {
    const tasks: VideoTask[] = Array.from({ length: 9 }, (_, i) => ({
      id: `library-${i}`, revision: 1, updatedAt: 1700000000000 + i,
      brief: { ...newVideoBrief(), name: `历史视频 ${i}`, request: `商品 ${i}` },
      script: '已确认的剧本', prompt: 'stored prompt', approved: true, status: 'approved',
    }))
    vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({
      tasks, config: {}, templates: [], root: '', configPath: '',
      dependencies: { python: '3.12', comfy: true, node: true, ffmpeg: true },
    })
    vi.mocked(api.videoStudioTask).mockResolvedValueOnce(tasks[0])
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '任务 9' })
    expect(screen.queryByRole('button', { name: '历史视频 0' })).toBeNull()
    expect(screen.getByRole('button', { name: '历史视频 8' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '任务 9' }))
    expect(screen.getByRole('heading', { name: '视频任务 9' })).toBeTruthy()
    fireEvent.change(screen.getByLabelText('搜索任务'), { target: { value: '历史视频 0' } })
    fireEvent.click(screen.getByRole('button', { name: '打开任务 历史视频 0' }))
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('get', { id: 'library-0' }))
    expect(await screen.findByText('stored prompt')).toBeTruthy()
  })

  beforeEach(() => localStorage.clear())
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
    expect(screen.getByRole('button', { name: '生成服务' }).textContent).toContain('请选择')
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findByRole('heading', { name: '内置运行环境' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: '安装 / 修复 Comfy 依赖' })).toBeNull()
  })

  it('does not toast bootstrap failures when opening the page', async () => {
    vi.mocked(api.videoStudioBootstrap).mockRejectedValueOnce(
      '视频操作失败，请检查配置、依赖和网络。远程任务可通过任务编号继续查询。',
    )
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '模板库 1' })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('does not report missing bundled files while the runtime check is pending', async () => {
    vi.mocked(api.videoStudioBootstrap).mockImplementationOnce(() => new Promise(() => {}))
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findAllByText('检测中…')).toHaveLength(5)
    expect(screen.queryByText('内置文件缺失')).toBeNull()
    expect(screen.getByRole('button', { name: '重新检查' })).toBeDisabled()
  })

  it('distinguishes a failed check from missing files and recovers on retry', async () => {
    vi.mocked(api.videoStudioBootstrap).mockRejectedValueOnce('unknown path')
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findAllByText('检测失败')).toHaveLength(5)
    expect(screen.getByText(/无法读取内置运行环境状态/)).toHaveTextContent('unknown path')
    expect(screen.queryByText('内置文件缺失')).toBeNull()
    expect(screen.queryByText(/重新安装 Dsivio/)).toBeNull()

    vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({
      tasks: [], templates: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.12.12', comfy: true, node: true, ffmpeg: true, analyzer: true, bundled: true },
    })
    fireEvent.click(screen.getByRole('button', { name: '重新检查' }))
    expect(await screen.findByText('3.12.12')).toBeTruthy()
    expect(screen.getAllByText('已就绪')).toHaveLength(4)
    expect(screen.queryByText(/无法读取内置运行环境状态/)).toBeNull()
    expect(screen.queryByText(/重新安装 Dsivio/)).toBeNull()
  })

  it('reports missing files only after a successful runtime check', async () => {
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findByText('3.14')).toBeTruthy()
    expect(screen.getAllByText('内置文件缺失')).toHaveLength(1)
    expect(screen.getByText('内置运行环境不完整，请重新安装 Dsivio。')).toBeTruthy()
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


describe('video workflow continuity', () => {
  beforeEach(() => { localStorage.clear(); vi.mocked(api.videoStudioTask).mockReset() })
  afterEach(() => localStorage.clear())

  it('keeps separate drafts when switching creation and analysis and after remount', async () => {
    const first = render(<VideoStudio />)
    fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: '让背包缓慢转动' } })
    fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
    fireEvent.change(screen.getByLabelText('重点分析什么（可选）'), { target: { value: '重点看开场' } })
    fireEvent.click(screen.getByRole('button', { name: '视频创作' }))
    expect(screen.getByLabelText('这次要拍什么')).toHaveValue('让背包缓慢转动')
    first.unmount()
    render(<VideoStudio />)
    expect(screen.getByLabelText('这次要拍什么')).toHaveValue('让背包缓慢转动')
    fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
    expect(screen.getByLabelText('重点分析什么（可选）')).toHaveValue('重点看开场')
  })

  it('carries the selected concept into planning without submitting generation', async () => {
    const brief = { name: '测试', mode: 'creation', request: '宣传背包', images: [], duration: 10, ratio: '9:16', route: '', resolution: '', language: 'zh-CN', source: '' }
    let task = { id: 'concept-task', revision: 1, updatedAt: 0, brief, script: '', prompt: '', approved: false, status: 'draft', concepts: ['细节特写', '生活场景', '动态展示'] }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief, task, script: '', step: 1, dirty: false } }))
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'save') task = { ...task, brief: input.brief as typeof brief, revision: task.revision + 1 }
      if (action === 'plan') task = { ...task, concepts: [], script: '0–10秒：生活场景展示', revision: task.revision + 1 }
      return task as never
    })
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: /生活场景/ }))
    await screen.findByText('0–10秒：生活场景展示')
    expect(api.videoStudioTask).toHaveBeenCalledWith('save', expect.objectContaining({ brief: expect.objectContaining({ selectedConcept: '生活场景' }) }))
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['save', 'plan'])
  })

  it('hands an analyzed reference to creation without requiring template save', async () => {
    const brief = { name: '参考', mode: 'analysis', request: '镜头节奏', images: ['/tmp/product.png'], duration: 10, ratio: '9:16', route: '', resolution: '', language: 'zh-CN', source: '/tmp/reference.mp4' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ remake: { brief, script: '0–5秒：展示商品', step: 1, dirty: false } }))
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '参考仿拍' }))
    fireEvent.click(screen.getByRole('button', { name: '确认结构，进入制作' }))
    expect(screen.getByText('已选模板：参考')).toBeInTheDocument()
    const draft = JSON.parse(localStorage.getItem('dsivio-video-drafts-v1')!).creation
    expect(draft.brief.images).toEqual(['/tmp/product.png'])
    expect(draft.brief.template.script).toBe('0–5秒：展示商品')
    expect(draft.brief.route).toBe('')
    expect(api.videoStudioTask).not.toHaveBeenCalled()
  })
})


describe('video confirmation and monitoring', () => {
  beforeEach(() => { localStorage.clear(); vi.mocked(api.videoStudioTask).mockReset() })
  afterEach(() => { vi.useRealTimers(); localStorage.clear() })
  function seed(task: VideoTask, step = 1) {
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: task.script, step, dirty: false } }))
  }
  function readyTask(): VideoTask {
    return { id: 'confirmed-task', revision: 1, updatedAt: 0, brief: { ...newVideoBrief(), route: 'grok', resolution: '720p' }, script: '0–10秒：商品展示', prompt: '', approved: false, status: 'draft' }
  }
  it('prepares and quotes automatically but submits only after the cost confirmation', async () => {
    let task = readyTask()
    seed(task)
    vi.mocked(api.videoStudioTask).mockImplementation(async action => {
      task = { ...task, revision: task.revision + 1 }
      if (action === 'approve') task.approved = true
      if (action === 'prepare') task.prompt = 'Approved product shot'
      if (action === 'quote') task.quote = { currency: 'USD', estimated_cost: { '720p': '1.40' }, note: '预计费用', at: Date.now() }
      if (action === 'submit') task.status = 'running'
      return task
    })
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '确认方案' }))
    await screen.findByText('预计 1.40 USD')
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['approve', 'prepare', 'quote'])
    fireEvent.click(screen.getByRole('button', { name: '确认费用并生成' }))
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('submit', expect.objectContaining({ confirmSpend: true })))
  })
  it('refreshes and polls a known running task without resubmission', async () => {
    vi.useFakeTimers()
    const task = { ...readyTask(), status: 'running', remote: { route: 'grok' as const, id: 'remote-1', base_url: 'https://api.x.ai' } }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockImplementation(async action => action === 'poll' ? { ...task, revision: 2, status: 'succeeded' } : task)
    render(<VideoStudio />)
    await act(async () => { await vi.advanceTimersByTimeAsync(8000) })
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get', 'poll'])
    await act(async () => { await vi.advanceTimersByTimeAsync(16000) })
    expect(vi.mocked(api.videoStudioTask).mock.calls).toHaveLength(2)
  })
  it('allows explicit recovery checks for an uncertain task with a known remote id', async () => {
    const task: VideoTask = { ...readyTask(), status: 'uncertain', remote: { route: 'grok', id: 'remote-1', base_url: 'https://api.x.ai' } }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockImplementation(async action => action === 'poll' ? { ...task, status: 'succeeded' } : task)
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '查询进度 / 恢复结果' }))
    await waitFor(() => expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get', 'poll']))
  })
  it('does not poll or resubmit a task with an uncertain submission', async () => {
    vi.useFakeTimers()
    seed({ ...readyTask(), status: 'uncertain' }, 2)
    render(<VideoStudio />)
    await act(async () => { await vi.advanceTimersByTimeAsync(24000) })
    expect(api.videoStudioTask).not.toHaveBeenCalled()
  })
})

it('shows the shared ComfyUI address and protects a local config edit from chat updates', async () => {
  localStorage.clear()
  const data = { tasks: [], templates: [], config: { comfy: { base_url: 'http://shared:8188' } }, root: '', configPath: '', dependencies: { python: '3.14', comfy: true, node: true, ffmpeg: true } }
  vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce(data)
  render(<VideoStudio />)
  fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
  const address = await screen.findByDisplayValue('http://shared:8188')
  fireEvent.change(address, { target: { value: 'http://local-edit:8188' } })
  vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({ ...data, config: { comfy: { base_url: 'http://chat-edit:8188' } } })
  fireEvent.focus(window)
  await screen.findByText('配置已在其他入口修改，请载入最新配置后再编辑。')
  expect(screen.getByDisplayValue('http://local-edit:8188')).toBeTruthy()
  expect(screen.getByRole('button', { name: '保存配置' })).toBeDisabled()
  fireEvent.click(screen.getByRole('button', { name: '载入最新配置' }))
  expect(await screen.findByDisplayValue('http://chat-edit:8188')).toBeTruthy()
  expect(screen.getByRole('button', { name: '保存配置' })).toBeEnabled()
})
