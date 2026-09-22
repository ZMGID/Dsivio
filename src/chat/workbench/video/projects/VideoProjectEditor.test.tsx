import { act, fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../../api/tauri'
import { open } from '@tauri-apps/plugin-dialog'
import { ShortsPage } from '../ShortsPage'
import { VideoAnalysisPage } from '../VideoAnalysisPage'
import { VideoClonePage } from '../VideoClonePage'
import { newVideoBrief, type VideoTask } from './types'

vi.mock('../../../../api/tauri', () => ({ isTauriRuntime: () => true, api: {
 workbenchVideoBootstrap: vi.fn(), workbenchVideoTask: vi.fn(), workbenchVideoImage: vi.fn(), workbenchVideoPreview: vi.fn(), workbenchVideoPoster: vi.fn(), studioTaskLibrary: vi.fn(async () => ({})),
} }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ revision: 0, value: null })) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }))
vi.mock('../../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(async () => ({ providers: [], workbenchMedia: { imageModels: [], videoModels: [] } })), subscribeSettings: vi.fn(() => () => {}) }))
vi.mock('../../../api', () => ({ chatApi: { getAssistants: vi.fn(async () => []) } }))

const brief = () => ({ ...newVideoBrief(), providerId: 'cloud', model: 'grok-imagine-video-1.5', route: 'grok' as const, resolution: '720p', request: '保留商品外观，缓慢旋转' })
let task: VideoTask
beforeEach(() => {
 vi.clearAllMocks(); localStorage.clear()
 task = { id: 'project', revision: 1, updatedAt: 1, brief: brief(), script: '', prompt: '', approved: false, status: 'draft' }
 vi.mocked(api.workbenchVideoBootstrap).mockResolvedValue({ tasks: [], templates: [], config: {}, root: '', configPath: '', dependencies: {} } as never)
 vi.mocked(api.workbenchVideoTask).mockImplementation(async (action, input) => {
  if (action === 'create') task = { ...task, brief: input.brief as typeof task.brief }
  if (action === 'save') task = { ...task, brief: input.brief as typeof task.brief, script: String(input.script || ''), approved: false, prompt: '' }
  if (action === 'approve') task = { ...task, approved: true, status: 'approved', prompt: task.script }
  if (action === 'plan' || action === 'analyze') task = { ...task, script: '镜头一：展示图片中的商品', status: 'draft' }
  if (action === 'submit') task = { ...task, status: 'running', remote: { id: 'receipt', route: 'grok', base_url: '' } }
  if (action !== 'get') task = { ...task, revision: task.revision + 1 }
  return { ...task }
 })
 vi.mocked(api.workbenchVideoImage).mockResolvedValue('data:image/png;base64,cG5n')
 vi.mocked(api.workbenchVideoPreview).mockResolvedValue('data:video/mp4;base64,bW92aWU=')
 vi.mocked(api.workbenchVideoPoster).mockRejectedValue(new Error('no poster'))
})
function draft() {
 localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: brief(), script: '', step: 0, dirty: true } }))
}
it('exposes distinct creation, analysis and remake input flows', async () => {
 render(<ShortsPage />)
 expect(screen.getByLabelText('商品素材投放区')).toBeVisible()
 expect(screen.queryByLabelText('参考视频投放区')).not.toBeInTheDocument()
 cleanup(); render(<VideoAnalysisPage />)
 expect(screen.getByLabelText('参考视频投放区')).toBeVisible()
 expect(screen.queryByLabelText('商品图片投放区')).not.toBeInTheDocument()
 cleanup(); render(<VideoClonePage />)
 expect(screen.getByLabelText('参考视频投放区')).toBeVisible()
 expect(screen.getByLabelText('商品图片投放区')).toBeVisible()
 expect(screen.queryByText('视频设置')).not.toBeInTheDocument()
})
it('keeps materials editable before selecting a model and persists it across reopening', async () => {
 vi.mocked(open).mockResolvedValue(['/tmp/product.png'])
 const page = render(<ShortsPage />)
 fireEvent.click(screen.getByRole('button', { name: '选择商品图片' }))
 expect(await screen.findByAltText('product.png')).toBeVisible()
 fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: '白鞋，展示鞋底' } })
 page.unmount(); render(<ShortsPage />)
 expect(screen.getByLabelText('这次要拍什么')).toHaveValue('白鞋，展示鞋底')
 expect(api.workbenchVideoTask).not.toHaveBeenCalled()
})
it('uses the original prompt without planning, then submits once and retains the receipt', async () => {
 draft(); render(<ShortsPage />)
 fireEvent.click(screen.getByRole('button', { name: '使用原提示词生成' }))
 const button = await screen.findByRole('button', { name: '生成视频' })
 await waitFor(() => expect(button).toBeEnabled())
 fireEvent.click(button); fireEvent.click(button)
 await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('submit', expect.anything()))
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.filter(([action]) => action === 'submit')).toHaveLength(1)
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'plan')).toBe(false)
 expect(task.brief).toMatchObject({ providerId: 'cloud', model: 'grok-imagine-video-1.5' })
 expect(task.prompt).toBe(brief().request)
 expect(task.remote?.id).toBe('receipt')
})
it('routes analysis through the project AI action without submitting a generation', async () => {
 render(<VideoAnalysisPage />)
 fireEvent.change(screen.getByLabelText('视频链接或本地路径'), { target: { value: '/tmp/reference.mp4' } })
 fireEvent.click(screen.getByRole('button', { name: '开始拆解' }))
 await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('analyze', expect.anything()))
 expect(task.brief.mode).toBe('analysis')
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'submit')).toBe(false)
})
it('ignores a late plan after a new draft is opened', async () => {
 draft(); let finish!: (value: VideoTask) => void
 const original = vi.mocked(api.workbenchVideoTask).getMockImplementation()!
 vi.mocked(api.workbenchVideoTask).mockImplementation((action, input) => action === 'plan' ? new Promise(resolve => { finish = resolve }) : original(action, input))
 render(<ShortsPage />)
 fireEvent.click(screen.getByRole('button', { name: '帮我设计视频' }))
 await waitFor(() => expect(finish).toBeDefined())
 fireEvent.click(screen.getByRole('button', { name: '新建' }))
 fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: '新的拍摄要求' } })
 await act(async () => finish({ ...task, script: '旧方案', revision: 20 }))
 expect(screen.getByLabelText('这次要拍什么')).toHaveValue('新的拍摄要求')
})

it('retries a definitively failed project as a new attempt and blocks repeated submission while running', async () => {
 task = { ...task, script: 'approved shot', prompt: 'approved shot', approved: true, status: 'failed', remote: { id: 'old-receipt', route: 'grok', base_url: '' } }
 localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
 const original = vi.mocked(api.workbenchVideoTask).getMockImplementation()!
 vi.mocked(api.workbenchVideoTask).mockImplementation(async (action, input) => {
  if (action === 'retry') { task = { ...task, revision: task.revision + 1, status: 'running', remote: { id: '', route: 'grok', base_url: '' } }; return task }
  return original(action, input)
 })
 render(<ShortsPage />)
 fireEvent.click(await screen.findByRole('button', { name: '重试生成' }))
 await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('retry', expect.objectContaining({ id: 'project' })))
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => ['create', 'submit', 'save'].includes(action))).toBe(false)
 const generate = await screen.findByRole('button', { name: '生成视频' })
 expect(generate).toBeDisabled()
 fireEvent.click(generate)
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.filter(([action]) => action === 'retry')).toHaveLength(1)
})

it('does not create a second project when a failed snapshot has resumed before retry', async () => {
 task = { ...task, script: 'approved shot', prompt: 'approved shot', approved: true, status: 'failed' }
 localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ creation: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
 vi.mocked(api.workbenchVideoTask).mockResolvedValue({ ...task, status: 'running', revision: 5 })
 render(<ShortsPage />)
 fireEvent.click(await screen.findByRole('button', { name: '重试生成' }))
 expect((await screen.findAllByText(/任务仍在运行，请查询原任务结果/))[0]).toBeVisible()
 expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => ['create', 'submit', 'retry'].includes(action))).toBe(false)
})
