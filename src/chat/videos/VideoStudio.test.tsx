import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { chatApi } from '../api'
import { api } from '../../api/tauri'
import VideoStudio from './VideoStudio'
import { newVideoBrief, type VideoTask } from './types'
import { readVideoTaskDraft } from './videoDrafts'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    onChatAssistantsChanged: vi.fn(async () => () => {}),
    studioTaskLibrary: vi.fn(async () => ({})),
    videoStudioBootstrap: vi.fn(async () => ({
      tasks: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.14', comfy: false, node: true, ffmpeg: true },
      templates: [{ id: 'chat-template', name: '聊天创建的参考模板', kind: 'reference', script: '真实参考镜头' }],
    })),
    videoStudioImage: vi.fn(async () => ''),
    videoStudioTask: vi.fn(),
    videoStudioPreview: vi.fn(async () => ''),
    videoStudioOpen: vi.fn(async () => {}),
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
  it('keeps navigation and header controls enabled while templates refresh', async () => {
    render(<VideoStudio />)
    const templates = await screen.findByRole('button', { name: '模板库 1' })
    let finish!: (value: Awaited<ReturnType<typeof api.videoStudioBootstrap>>) => void
    vi.mocked(api.videoStudioBootstrap).mockReturnValueOnce(new Promise(resolve => { finish = resolve }))
    fireEvent.click(templates)
    expect(screen.getByRole('button', { name: '视频设置' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '刷新共享模板' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '导入参考模板' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
    await act(async () => finish({ tasks: [], templates: [], config: {}, root: '', configPath: '', dependencies: { python: '3.12', comfy: true, node: true, ffmpeg: true } }))
    expect(screen.getByRole('heading', { name: '视频拆解' })).toBeTruthy()
    expect(screen.getByRole('button', { name: '视频设置' })).toBeEnabled()
  })

  it('does not dim unrelated controls or reopen a task after navigating away', async () => {
    const target: VideoTask = { id: 'slow-task', revision: 1, updatedAt: 1,
      brief: { ...newVideoBrief(), name: '慢任务' }, script: '旧任务剧本', prompt: '', approved: false, status: 'draft' }
    vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({ tasks: [target], templates: [], config: {}, root: '', configPath: '', dependencies: { python: '3.12', comfy: true, node: true, ffmpeg: true } })
    let finish!: (value: VideoTask) => void
    vi.mocked(api.videoStudioTask).mockReturnValueOnce(new Promise(resolve => { finish = resolve }))
    render(<VideoStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '任务 1' }))
    const row = screen.getByRole('button', { name: '打开任务 慢任务' })
    fireEvent.click(row)
    expect(screen.getByRole('button', { name: '刷新任务' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '视频设置' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
    await act(async () => finish(target))
    expect(screen.getByRole('heading', { name: '视频拆解' })).toBeTruthy()
    expect(screen.queryByText('旧任务剧本')).toBeNull()
  })

  it.each([
    { route: '', resolution: '', reason: '请选择生成服务' },
    { route: 'grok', resolution: '', reason: '请选择当前服务支持的生成清晰度' },
  ])('blocks planning and step shortcuts with missing selections: $reason', async ({ route, resolution, reason }) => {
    const brief = { ...newVideoBrief(), request: '展示背包', route, resolution }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief, script: '', step: 0, dirty: true } }))
    vi.mocked(api.videoStudioTask).mockClear()
    render(<VideoStudio />)
    const start = await screen.findByRole('button', { name: '帮我设计视频' })
    expect(start).toBeDisabled()
    expect(screen.getByText(reason)).toBeTruthy()
    const plan = screen.getByRole('tab', { name: /拍摄方案/ })
    const output = screen.getByRole('tab', { name: /生成与成片/ })
    expect(plan).toBeDisabled()
    expect(output).toBeDisabled()
    fireEvent.click(start)
    fireEvent.click(plan)
    fireEvent.click(output)
    expect(api.videoStudioTask).not.toHaveBeenCalled()
  })
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

  it('opens a selected task without an implicit save or a layout-shifting busy notice', async () => {
    const current: VideoTask = {
      id: 'current-draft', revision: 2, updatedAt: 1700000000001,
      brief: { ...newVideoBrief(), name: '当前任务', request: '未保存的本地修改' },
      script: '本地剧本', prompt: '', approved: true, status: 'approved',
    }
    const target: VideoTask = {
      id: 'completed-task', revision: 4, updatedAt: 1700000000002,
      brief: { ...newVideoBrief(), name: '已完成任务', request: '打开我' },
      script: '已完成剧本', prompt: 'target prompt', approved: true, status: 'succeeded', output: '/tmp/video.mp4',
    }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({
      creation: { brief: current.brief, task: current, script: current.script, step: 1, dirty: true },
    }))
    vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({
      tasks: [target, current], config: {}, templates: [], root: '', configPath: '',
      dependencies: { python: '3.12', comfy: true, node: true, ffmpeg: true },
    })
    vi.mocked(api.videoStudioTask).mockReset()
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'save') throw new Error('旧任务版本冲突')
      return input.id === current.id ? { ...current, brief: { ...current.brief, request: '服务器旧内容' }, script: '服务器旧剧本' } : target
    })

    render(<VideoStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '任务 2' }))
    fireEvent.click(screen.getByRole('button', { name: '打开任务 已完成任务' }))

    expect(screen.queryByText('打开任务…')).toBeNull()
    expect(await screen.findByText('target prompt')).toBeTruthy()
    expect(api.videoStudioTask).toHaveBeenCalledWith('get', { id: 'completed-task' })
    expect(api.videoStudioTask).not.toHaveBeenCalledWith('save', expect.anything())
    fireEvent.click(screen.getByRole('button', { name: '在文件管理器中显示' }))
    await waitFor(() => expect(api.videoStudioOpen).toHaveBeenCalledWith('completed-task', 'reveal'))
    expect(readVideoTaskDraft(current.id)?.brief.request).toBe('未保存的本地修改')
    fireEvent.click(screen.getByRole('button', { name: '任务 2' }))
    fireEvent.click(screen.getByRole('button', { name: '打开任务 当前任务' }))
    await waitFor(() => expect(readVideoTaskDraft(current.id)?.dirty).toBe(true))
    expect(await screen.findByText('本地剧本')).toBeTruthy()
    expect(readVideoTaskDraft(current.id)?.brief.request).toBe('未保存的本地修改')
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
    expect(screen.queryByText(/重新安装 dsivio/)).toBeNull()

    vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({
      tasks: [], templates: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.12.12', comfy: true, node: true, ffmpeg: true, analyzer: true, bundled: true },
    })
    fireEvent.click(screen.getByRole('button', { name: '重新检查' }))
    expect(await screen.findByText('3.12.12')).toBeTruthy()
    expect(screen.getAllByText('已就绪')).toHaveLength(4)
    expect(screen.queryByText(/无法读取内置运行环境状态/)).toBeNull()
    expect(screen.queryByText(/重新安装 dsivio/)).toBeNull()
  })

  it('reports missing files only after a successful runtime check', async () => {
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findByText('3.14')).toBeTruthy()
    expect(screen.getAllByText('内置文件缺失')).toHaveLength(1)
    expect(screen.getByText('内置运行环境不完整，请重新安装 dsivio。')).toBeTruthy()
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
    expect(toast.className).toContain('studio-toast')
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

  it('imports remake product images on the goods drop zone without replacing the reference video', async () => {
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '参考仿拍' }))
    const images = await screen.findByLabelText('商品图片投放区')
    const video = screen.getByLabelText('参考视频投放区')
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    fireEvent.dragEnter(images)
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(images).toHaveClass('is-drop-active'))
    expect(video).not.toHaveClass('is-drop-active')
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\goods\\front.png'] } })
    expect(await screen.findByText('front.png')).toBeTruthy()
    expect(screen.getByText('把参考视频拖到这里')).toBeTruthy()
    expect(screen.getByRole('button', { name: '继续添加商品图片' })).toBeTruthy()
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
    const brief = { name: '测试', mode: 'creation', request: '宣传背包', images: [], duration: 10, ratio: '9:16', route: 'grok', resolution: '720p', language: 'zh-CN', source: '' }
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
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get', 'save', 'plan'])
  })

  it('recreates a deleted remake task from the retained materials before analysis', async () => {
    const brief = { ...newVideoBrief(), mode: 'analysis' as const, images: ['/tmp/product.png'], source: '/tmp/reference.mp4' }
    const oldTask: VideoTask = { id: 'deleted', revision: 4, updatedAt: 0, brief, script: '保留的结构', prompt: '', approved: false, status: 'draft' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ remake: { brief, task: oldTask, script: oldTask.script, step: 0, dirty: true } }))
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'get') throw new Error('VIDEO_TASK_NOT_FOUND: 视频任务不存在或已删除')
      if (action === 'create') return { ...oldTask, id: 'replacement', revision: 1, script: '' }
      if (action === 'save') return { ...oldTask, id: 'replacement', revision: 2, script: input.script as string }
      return { ...oldTask, id: 'replacement', revision: 3, script: '重新分析结果' }
    })
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '参考仿拍' }))
    fireEvent.click(screen.getByRole('button', { name: '分析参考并适配商品' }))
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('analyze', { id: 'replacement', revision: 2, confirmSpend: false }))
    expect(api.videoStudioTask).toHaveBeenCalledWith('create', { brief: expect.objectContaining({ source: brief.source, images: brief.images }) })
    expect(api.videoStudioTask).toHaveBeenCalledWith('save', expect.objectContaining({ id: 'replacement', script: oldTask.script }))
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get', 'create', 'save', 'analyze'])
    await waitFor(() => expect(JSON.parse(localStorage.getItem('dsivio-video-drafts-v1')!).remake.task.id).toBe('replacement'))
  })

  it('does not recreate a task when reading it fails for another reason', async () => {
    const brief = { ...newVideoBrief(), mode: 'analysis' as const, source: '/tmp/reference.mp4', images: ['/tmp/product.png'] }
    const task: VideoTask = { id: 'existing', revision: 1, updatedAt: 0, brief, script: '', prompt: '', approved: false, status: 'draft' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ remake: { brief, task, script: '', step: 0, dirty: true } }))
    vi.mocked(api.videoStudioTask).mockRejectedValue(new Error('无法读取任务目录'))
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '参考仿拍' }))
    fireEvent.click(screen.getByRole('button', { name: '分析参考并适配商品' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('无法读取任务目录')
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get'])
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
  it.each(['grok', 'minimax', 'comfy'] as const)('%s uses the original prompt without planning or conversion', async route => {
    const original = '  固定机位，产品保持参考图外观。\nSay exactly: "Hello".  '
    let task: VideoTask = { ...readyTask(), brief: { ...newVideoBrief(), route,
      resolution: route === 'grok' ? '720p' : route === 'minimax' ? '768P' : '1', assistantId: 'asst_deleted', request: original }, script: '旧的拍摄方案' }
    seed(task, 0)
    vi.mocked(api.videoStudioBootstrap).mockResolvedValue({ tasks: [], config: {}, templates: [], root: '', configPath: '', dependencies: { python: '', comfy: true, node: true, ffmpeg: true } })
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'save') task = { ...task, revision: task.revision + 1, brief: input.brief as VideoTask['brief'], script: input.script as string, approved: false, prompt: '' }
      if (action === 'approve') task = { ...task, revision: task.revision + 1, approved: true, status: 'approved', prompt: route === 'grok' ? task.script : '' }
      if (action === 'prompt_result') task = { ...task, revision: task.revision + 1, prompt: input.prompt as string }
      if (action === 'quote') task = { ...task, revision: task.revision + 1, quote: { note: '原文生成报价', at: 1 } }
      if (action === 'submit') task = { ...task, status: 'running' }
      return task
    })
    render(<VideoStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '使用原提示词生成' }))
    await screen.findByText('原文生成报价')
    expect(task.script).toBe(original)
    expect(task.prompt).toBe(original)
    expect(screen.getByRole('tab', { name: /生成与成片/ })).toHaveAttribute('aria-selected', 'true')
    const calls = vi.mocked(api.videoStudioTask).mock.calls.map(([action]) => action)
    expect(calls).toEqual(route === 'grok' ? ['get', 'save', 'approve', 'quote'] : ['get', 'save', 'approve', 'prompt_result', 'quote'])
    fireEvent.click(screen.getByRole('button', { name: route === 'comfy' ? '开始生成' : '生成视频' }))
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('submit', expect.objectContaining({ confirmSpend: true })))
    expect(task.prompt).toBe(original)
  })
  it('requires a nonempty original prompt even when a template was selected', async () => {
    const task = readyTask()
    task.brief.request = '   '
    task.brief.template = { id: 'template', name: '模板', script: '模板内容' }
    seed(task, 0)
    render(<VideoStudio />)
    expect(await screen.findByRole('button', { name: '使用原提示词生成' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '帮我设计视频' })).toBeEnabled()
  })
  it('revises step two using the latest manually edited plan and waits for confirmation', async () => {
    let task = readyTask()
    seed(task)
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'save') task = { ...task, revision: 2, script: input.script as string }
      if (action === 'revise') task = { ...task, revision: 3, script: '新方案：鞋子特写开场', approved: false, prompt: '' }
      return task
    })
    render(<VideoStudio />)
    const revise = await screen.findByRole('button', { name: '让 AI 修改' })
    expect(revise).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: '编辑全文' }))
    fireEvent.change(screen.getByLabelText('视频方案'), { target: { value: '手动修改：保留客厅场景' } })
    fireEvent.change(screen.getByLabelText('修改要求'), { target: { value: '  开头改成鞋子特写  ' } })
    fireEvent.click(revise)
    expect(await screen.findByText('新方案：鞋子特写开场')).toBeTruthy()
    expect(api.videoStudioTask).toHaveBeenCalledWith('save', expect.objectContaining({ script: '手动修改：保留客厅场景' }))
    expect(api.videoStudioTask).toHaveBeenCalledWith('revise', { id: task.id, revision: 2, note: '开头改成鞋子特写' })
    expect(screen.getByLabelText('修改要求')).toHaveValue('')
    expect(screen.getByRole('button', { name: '确认方案' })).toBeEnabled()
    expect(vi.mocked(api.videoStudioTask).mock.calls.some(([action]) => ['approve', 'prepare', 'submit'].includes(action))).toBe(false)
  })
  it('keeps the existing plan and revision instructions when AI revision fails', async () => {
    const task = readyTask()
    seed(task)
    vi.mocked(api.videoStudioTask).mockRejectedValue(new Error('服务暂不可用'))
    render(<VideoStudio />)
    fireEvent.change(await screen.findByLabelText('修改要求'), { target: { value: '减少人物镜头' } })
    fireEvent.click(screen.getByRole('button', { name: '让 AI 修改' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('服务暂不可用')
    expect(screen.getByLabelText('修改要求')).toHaveValue('减少人物镜头')
    expect(screen.getByText(task.script)).toBeTruthy()
    expect(screen.getByRole('button', { name: '让 AI 修改' })).toBeEnabled()
  })
  it.each(['grok', 'minimax'] as const)('%s only converts H3 and submits after cost confirmation', async route => {
    let task = readyTask()
    task.brief.route = route
    task.brief.resolution = route === 'minimax' ? '768P' : '720p'
    seed(task)
    vi.mocked(api.videoStudioTask).mockImplementation(async action => {
      task = { ...task, revision: task.revision + 1 }
      if (action === 'approve') {
        task.approved = true
        if (route === 'grok') task.prompt = task.script
      }
      if (action === 'prepare') task.prompt = 'Approved product shot'
      if (action === 'quote') task.quote = { currency: 'USD', estimated_cost: { [task.brief.resolution]: '1.40' }, note: '预计费用', at: Date.now() }
      if (action === 'submit') task.status = 'running'
      return task
    })
    render(<VideoStudio />)
    fireEvent.click(screen.getByRole('button', { name: '确认方案' }))
    await screen.findByText('官方参考 1.40 USD')
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(route === 'grok' ? ['get', 'approve', 'quote'] : ['get', 'approve', 'prepare', 'quote'])
    fireEvent.click(screen.getByRole('button', { name: '生成视频' }))
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('submit', expect.objectContaining({ confirmSpend: true })))
  })
  it('allows generation when the shared catalog has no price', async () => {
    const task: VideoTask = { ...readyTask(), approved: true, status: 'approved', prompt: 'Approved shot', quote: { pricingStatus: 'unknown', note: '暂无价格数据，以供应商实际计费为准。', at: Date.now() } }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockResolvedValue({ ...task, status: 'running' })
    render(<VideoStudio />)
    const button = await screen.findByRole('button', { name: '生成视频' })
    expect(button).toBeEnabled()
    expect(screen.getByText('以供应商实际计费为准')).toBeTruthy()
    fireEvent.click(button)
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('submit', expect.objectContaining({ confirmSpend: true })))
  })
  it('can continue an already approved task that has no quote', async () => {
    const task: VideoTask = { ...readyTask(), approved: true, status: 'approved', prompt: 'Approved shot' }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockImplementation(async action => ({ ...task,
      quote: { pricingStatus: 'unknown', note: '按供应商实际计费', at: Date.now() / 1000 },
      status: action === 'submit' ? 'running' : 'approved',
    }))
    render(<VideoStudio />)
    const button = await screen.findByRole('button', { name: '生成视频' })
    expect(button).toBeEnabled()
    fireEvent.click(button)
    await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('submit', expect.objectContaining({ confirmSpend: true })))
    expect(vi.mocked(api.videoStudioTask).mock.calls.map(c => c[0])).toEqual(['get', 'submit'])
  })
  it('keeps a rejected request editable and offers retry without asking for an ID', async () => {
    const task: VideoTask = { ...readyTask(), approved: true, status: 'approved', prompt: 'Approved shot',
      submission: { state: 'rejected', httpStatus: 401, reason: 'API Key 无效，请检查密钥。', retryable: true } }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockResolvedValue({ ...task, quote: { note: '参考价', at: Date.now() / 1000 }, status: 'running' })
    render(<VideoStudio />)
    expect(await screen.findByText(/API Key 无效/)).toBeTruthy()
    expect(screen.queryByText('补录远程任务编号')).toBeNull()
    const retry = screen.getByRole('button', { name: '重试生成' })
    expect(retry).toBeEnabled()
    fireEvent.click(retry)
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

  it('puts the finished video first and revises the shooting plan on the same task', async () => {
    const task: VideoTask = {
      ...readyTask(),
      approved: true,
      status: 'succeeded',
      prompt: 'Approved shot',
      output: '/tmp/video.mp4',
      remote: { route: 'grok', id: 'remote-1', base_url: 'https://api.x.ai' },
    }
    seed(task, 2)
    vi.mocked(api.videoStudioTask).mockImplementation(async (action) => {
      if (action === 'revise') {
        return {
          ...task,
          revision: task.revision + 1,
          status: 'draft',
          approved: false,
          prompt: '',
          script: '修订后的方案：书包完整入画',
          remote: undefined,
        }
      }
      return task
    })
    render(<VideoStudio />)
    const result = await screen.findByRole('heading', { name: '生成结果' })
    const spec = screen.getByRole('heading', { name: '生成规格' })
    expect(result.compareDocumentPosition(spec) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByRole('heading', { name: '保存为成片验证模板' })).toBeTruthy()
    fireEvent.change(screen.getByLabelText('需要修改的地方'), { target: { value: '书包要完整入画' } })
    fireEvent.click(screen.getByRole('button', { name: '按意见改写拍摄方案' }))
    expect(await screen.findByText('修订后的方案：书包完整入画')).toBeTruthy()
    expect(api.videoStudioTask).toHaveBeenCalledWith('revise', expect.objectContaining({
      id: 'confirmed-task',
      note: '书包要完整入画',
    }))
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

it('continues analysis across tasks and a new chat without overwriting another draft', async () => {
  localStorage.clear()
  const source: VideoTask = {
    id: 'background-analysis', revision: 1, updatedAt: 1,
    brief: { ...newVideoBrief('analysis'), name: '后台拆解', source: '/tmp/demo.mp4' },
    script: '', prompt: '', approved: false, status: 'draft',
  }
  let finish!: (task: VideoTask) => void
  const pending = new Promise<VideoTask>(resolve => { finish = resolve })
  vi.mocked(api.videoStudioTask).mockImplementation(async action => action === 'analyze' ? pending : source)
  const { ChatRouteKeepAlive } = await import('../ChatRouteKeepAlive')
  const page = () => <ChatRouteKeepAlive activeKey="videos"><VideoStudio /></ChatRouteKeepAlive>
  const { rerender } = render(page())
  fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
  fireEvent.change(screen.getByPlaceholderText('粘贴视频链接，或把视频拖到这里'), { target: { value: source.brief.source } })
  fireEvent.click(screen.getByRole('button', { name: '开始拆解' }))
  await waitFor(() => expect(api.videoStudioTask).toHaveBeenCalledWith('analyze', expect.objectContaining({ id: source.id })))
  fireEvent.click(screen.getByRole('button', { name: /任务 \d/ }))
  expect(screen.getByRole('heading', { name: /视频任务/ })).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '视频创作' }))
  fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: '另一份创作草稿' } })
  rerender(<ChatRouteKeepAlive activeKey="conversation"><main>新聊天</main></ChatRouteKeepAlive>)
  await act(async () => { finish({ ...source, revision: 2, script: '后台拆解完成的镜头结构' }); await pending })
  rerender(page())
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('另一份创作草稿')
  fireEvent.click(screen.getByRole('button', { name: '视频拆解' }))
  expect(await screen.findByText('后台拆解完成的镜头结构')).toBeInTheDocument()
  expect(vi.mocked(api.videoStudioTask).mock.calls.filter(([action]) => action === 'analyze')).toHaveLength(1)
  localStorage.clear()
})


it('keeps unsaved video requirements when another task revision arrives on focus', async () => {
  localStorage.clear()
  const task: VideoTask = { id: 'editing', revision: 1, updatedAt: 0, brief: { ...newVideoBrief(), request: 'saved request' }, script: '', prompt: '', status: 'draft', approved: false }
  const bootstrap = { tasks: [task], templates: [], config: {}, root: '', configPath: '', dependencies: { python: '3', comfy: true, node: true, ffmpeg: true } }
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: '', step: 0, dirty: false } }))
  vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce(bootstrap)
  render(<VideoStudio />)
  await waitFor(() => expect(screen.getByLabelText('这次要拍什么')).toHaveValue('saved request'))
  fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: 'local edit' } })
  vi.mocked(api.videoStudioBootstrap).mockResolvedValueOnce({ ...bootstrap, tasks: [{ ...task, revision: 2 }] })
  await act(async () => { fireEvent.focus(window) })
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('local edit')
  localStorage.clear()
})

describe('video settings and stable result controls', () => {
  beforeEach(() => {
    localStorage.clear()
    vi.mocked(api.videoStudioPreview).mockReset().mockResolvedValue('data:video/mp4;base64,AAAA')
    vi.mocked(api.videoStudioOpen).mockReset().mockResolvedValue(undefined)
  })
  it('inherits the previous task service and resolution when starting a new task', async () => {
    const brief = { ...newVideoBrief(), route: 'grok' as const, resolution: '720p', request: 'old request', inputMode: 'image' as const }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief, script: 'old script', step: 0, dirty: false } }))
    vi.mocked(api.videoStudioBootstrap).mockResolvedValue({ tasks: [], config: {}, templates: [], root: '', configPath: '', dependencies: { python: '', comfy: true, node: true, ffmpeg: true } })
    const view = render(<VideoStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '新建' }))
    expect(screen.getByRole('button', { name: '生成服务' })).toHaveTextContent('Grok Video')
    expect(screen.getByRole('button', { name: '生成清晰度' })).toHaveTextContent('720p')
    expect(screen.getByRole('button', { name: '生成模式' })).toHaveTextContent('按素材自动匹配')
    expect(screen.queryByDisplayValue('old request')).toBeNull()
    view.unmount()
    localStorage.removeItem('dsivio-video-drafts-v1')
    render(<VideoStudio />)
    expect(await screen.findByRole('button', { name: '生成服务' })).toHaveTextContent('Grok Video')
    expect(screen.getByRole('button', { name: '生成清晰度' })).toHaveTextContent('720p')
  })
  it('keeps the video element when opening files and refreshing task metadata', async () => {
    let task: VideoTask = { id: 'stable-output', revision: 1, updatedAt: 1, brief: { ...newVideoBrief(), route: 'grok', resolution: '720p', request: 'product' }, script: 'script', prompt: 'script', approved: true, status: 'succeeded', error: '', output: '/tmp/output.mp4' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
    vi.mocked(api.videoStudioBootstrap).mockImplementation(async () => ({ tasks: [task], config: {}, templates: [], root: '', configPath: '', dependencies: { python: '', comfy: true, node: true, ffmpeg: true } }))
    const { container } = render(<VideoStudio />)
    await waitFor(() => expect(container.querySelector('video')).not.toBeNull())
    const player = container.querySelector('video')
    fireEvent.click(screen.getByRole('button', { name: '打开本地成片' }))
    await waitFor(() => expect(api.videoStudioOpen).toHaveBeenCalledWith('stable-output'))
    await act(async () => {})
    fireEvent.click(screen.getByRole('button', { name: '在文件管理器中显示' }))
    await waitFor(() => expect(api.videoStudioOpen).toHaveBeenCalledWith('stable-output', 'reveal'))
    task = { ...task, revision: 2, updatedAt: 2 }
    fireEvent.focus(window)
    await waitFor(() => expect(JSON.parse(localStorage.getItem('dsivio-video-drafts-v1')!).creation.task.revision).toBe(2))
    expect(container.querySelector('video')).toBe(player)
    expect(api.videoStudioPreview).toHaveBeenCalledTimes(1)
  })
  it('shows preview errors instead of silently removing the player', async () => {
    const task: VideoTask = { id: 'preview-failure', revision: 1, updatedAt: 1, brief: { ...newVideoBrief(), route: 'grok', resolution: '720p', request: 'product' }, script: 'script', prompt: 'script', approved: true, status: 'succeeded', output: '/tmp/missing.mp4' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
    vi.mocked(api.videoStudioPreview).mockRejectedValueOnce('成片文件不存在')
    vi.mocked(api.videoStudioBootstrap).mockResolvedValue({ tasks: [task], config: {}, templates: [], root: '', configPath: '', dependencies: { python: '', comfy: true, node: true, ffmpeg: true } })
    render(<VideoStudio />)
    expect(await screen.findByText('成片文件不存在')).toBeTruthy()
    expect(screen.getByRole('button', { name: '打开本地成片' })).toBeEnabled()
  })
})


describe('video planning assistants', () => {
  beforeEach(() => { localStorage.clear(); vi.mocked(api.videoStudioTask).mockReset() })
  afterEach(() => { localStorage.clear(); vi.mocked(chatApi.getAssistants).mockResolvedValue([]) })
  it('selects only video assistants, persists selection, and saves it for planning and revisions', async () => {
    vi.mocked(chatApi.getAssistants).mockResolvedValue([
      { id: 'asst_builtin_video_prompt', name: '通用视频', category: 'video', created_at: 0, updated_at: 0 },
      { id: 'asst_builtin_video_product', name: '电商产品展示', category: 'video', installed: false, created_at: 0, updated_at: 0 },
      { id: 'asst_writer', name: '写作助手', category: 'writing', created_at: 0, updated_at: 0 },
      { id: 'asst_archived', name: '归档助手', category: 'video', archived: true, created_at: 0, updated_at: 0 },
    ])
    let task: VideoTask = { id: 'assistant-task', revision: 1, updatedAt: 1, brief: { ...newVideoBrief(), request: '慢慢展示产品', route: 'grok', resolution: '720p' }, script: '', prompt: '', approved: false, status: 'draft' }
    localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: '', step: 0, dirty: false } }))
    vi.mocked(api.videoStudioBootstrap).mockResolvedValue({ tasks: [], config: {}, templates: [], root: '', configPath: '', dependencies: { python: '', comfy: true, node: true, ffmpeg: true } })
    vi.mocked(api.videoStudioTask).mockImplementation(async (action, input) => {
      if (action === 'save') task = { ...task, revision: task.revision + 1, brief: input.brief as VideoTask['brief'] }
      if (action === 'plan') task = { ...task, script: '展示方案', revision: task.revision + 1 }
      if (action === 'revise') task = { ...task, script: '修改后的方案', revision: task.revision + 1 }
      return task
    })
    const view = render(<VideoStudio />)
    expect(await screen.findByRole('button', { name: '提示词助手' })).toHaveTextContent('通用视频（默认）')
    await waitFor(() => expect(chatApi.getAssistants).toHaveBeenCalled())
    fireEvent.click(screen.getByRole('button', { name: '提示词助手' }))
    await screen.findByRole('option', { name: '电商产品展示' })
    expect(screen.queryByRole('option', { name: '写作助手' })).toBeNull()
    expect(screen.queryByRole('option', { name: '归档助手' })).toBeNull()
    fireEvent.click(screen.getByRole('option', { name: '电商产品展示' }))
    fireEvent.click(screen.getByRole('button', { name: '帮我设计视频' }))
    expect(await screen.findByText('展示方案')).toBeTruthy()
    expect(task.brief.assistantId).toBe('asst_builtin_video_product')
    fireEvent.change(screen.getByLabelText('修改要求'), { target: { value: '动作慢一点' } })
    fireEvent.click(screen.getByRole('button', { name: '让 AI 修改' }))
    expect(await screen.findByText('修改后的方案')).toBeTruthy()
    expect(task.brief.assistantId).toBe('asst_builtin_video_product')
    expect(api.videoStudioTask).toHaveBeenCalledWith('revise', expect.objectContaining({ id: task.id, note: '动作慢一点' }))
    view.unmount()
    localStorage.removeItem('dsivio-video-drafts-v1')
    render(<VideoStudio />)
    await waitFor(() => expect(screen.getByRole('button', { name: '提示词助手' })).toHaveTextContent('电商产品展示'))
  })
})
