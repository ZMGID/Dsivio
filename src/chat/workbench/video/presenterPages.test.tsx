import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { open } from '@tauri-apps/plugin-dialog'
import { AvatarPage } from './AvatarPage'
import { DramaPage } from './DramaPage'
import { newVideoBrief, type VideoTask } from './projects/types'

vi.mock('../content/RolePicker', () => ({
  useRoles: () => ({ roles: [{ id: 'r1', name: '主播', images: ['/roles/host.png'] }], error: '' }),
  rolesToReferenceImages: (roles: Array<{ images: string[] }>) => roles.flatMap(role => role.images),
  RolePicker: ({ value, onChange }: { value: string[]; onChange: (ids: string[]) => void }) => (
    <button type="button" aria-pressed={value.includes('r1')} onClick={() => onChange(value.includes('r1') ? [] : ['r1'])}>选择角色</button>
  ),
}))
vi.mock('../../../api/tauri', () => ({ isTauriRuntime: () => true, api: {
  workbenchVideoBootstrap: vi.fn(), workbenchVideoTask: vi.fn(), workbenchVideoImage: vi.fn(), workbenchVideoPreview: vi.fn(), workbenchVideoPoster: vi.fn(), studioTaskLibrary: vi.fn(async () => ({})),
} }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ revision: 0, value: null })) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(async () => ({ providers: [], workbenchMedia: { imageModels: [], videoModels: [] } })), subscribeSettings: vi.fn(() => () => {}) }))
vi.mock('../../api', () => ({ chatApi: { getAssistants: vi.fn(async () => []) } }))

const avatarBrief = () => ({ ...newVideoBrief('avatar'), providerId: 'cloud', model: 'grok-imagine-video-1.5', route: 'grok' as const, resolution: '720p', images: ['/tmp/sku.png'], roleIds: ['r1'], roleImages: ['/roles/host.png'], request: '对着镜头介绍这双鞋' })
const dramaBrief = () => ({ ...newVideoBrief('drama'), providerId: 'cloud', model: 'grok-imagine-video-1.5', route: 'grok' as const, resolution: '720p', images: ['/tmp/sku.png'], roleIds: ['r1'], roleImages: ['/roles/host.png'], dramaStyle: 'twist', request: '一个反转故事' })
let task: VideoTask

beforeEach(() => {
  vi.clearAllMocks()
  localStorage.clear()
  task = { id: 'project', revision: 1, updatedAt: 1, brief: avatarBrief(), script: '', prompt: '', approved: false, status: 'draft' }
  vi.mocked(api.workbenchVideoBootstrap).mockResolvedValue({ tasks: [], templates: [], config: {}, root: '', configPath: '', dependencies: {} } as never)
  vi.mocked(api.workbenchVideoTask).mockImplementation(async (action, input) => {
    if (action === 'create') task = { ...task, brief: input.brief as VideoTask['brief'] }
    if (action === 'save') task = { ...task, brief: input.brief as VideoTask['brief'], script: String(input.script || ''), approved: false, prompt: '' }
    if (action === 'approve') task = { ...task, approved: true, status: task.shots?.some(shot => shot.status === 'failed') ? 'failed' : 'approved', prompt: task.script }
    if (action === 'plan') task = { ...task, script: task.brief.mode === 'drama' ? '## 镜头 1\n开场\n\n## 镜头 2\n收尾' : '主播介绍商品', status: 'draft' }
    if (action === 'prepare' || action === 'prompt_result') task = { ...task, prompt: task.script, approved: true }
    if (action === 'submit') task = { ...task, status: 'running', remote: { id: 'receipt', route: 'grok', base_url: '' } }
    if (action === 'retry') task = { ...task, status: 'running', revision: task.revision }
    if (action !== 'get') task = { ...task, revision: task.revision + 1 }
    return { ...task }
  })
  vi.mocked(api.workbenchVideoImage).mockResolvedValue('data:image/png;base64,cG5n')
  vi.mocked(api.workbenchVideoPreview).mockResolvedValue('data:video/mp4;base64,bW92aWU=')
  vi.mocked(api.workbenchVideoPoster).mockRejectedValue(new Error('no poster'))
  vi.mocked(open).mockResolvedValue(['/tmp/sku.png'])
})

async function assemble(page: 'avatar' | 'drama') {
  render(page === 'avatar' ? <AvatarPage /> : <DramaPage />)
  fireEvent.click(screen.getByRole('button', { name: '选择商品图片' }))
  fireEvent.click(await screen.findByRole('button', { name: '选择角色' }))
  fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: page === 'avatar' ? '对着镜头介绍这双鞋' : '一个反转故事' } })
  if (page === 'drama') fireEvent.click(screen.getByRole('button', { name: '甜宠恋爱' }))
  fireEvent.click(screen.getByRole('button', { name: '帮我设计视频' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('plan', expect.anything()))
  const saved = vi.mocked(api.workbenchVideoTask).mock.calls.filter(([action]) => action === 'save' || action === 'create').at(-1)
  return saved?.[1].brief as Record<string, unknown>
}

it('assembles an avatar script request with role and product references', async () => {
  const brief = await assemble('avatar')
  expect(screen.getByRole('heading', { name: '真人带货视频' })).toBeVisible()
  expect(brief).toMatchObject({
    mode: 'avatar',
    roleIds: ['r1'],
    roleImages: ['/roles/host.png'],
    images: ['/tmp/sku.png'],
    request: '对着镜头介绍这双鞋',
  })
  cleanup()
})

it('keeps an avatar draft when the page is left and reopened', async () => {
  const page = render(<AvatarPage />)
  fireEvent.change(screen.getByLabelText('这次要拍什么'), { target: { value: '留下的口播要求' } })
  fireEvent.click(screen.getByRole('button', { name: '选择角色' }))
  page.unmount()
  render(<AvatarPage />)
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('留下的口播要求')
  expect(screen.getByRole('button', { name: '选择角色' })).toHaveAttribute('aria-pressed', 'true')
  expect(api.workbenchVideoTask).not.toHaveBeenCalled()
})

it('retries a failed avatar generation without planning again', async () => {
  task = { ...task, brief: avatarBrief(), script: '已确认口播', prompt: '已确认口播', approved: true, status: 'failed', error: '供应商拒绝', remote: { id: 'old', route: 'grok', base_url: '' } }
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ avatar: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
  render(<AvatarPage />)
  expect(await screen.findByRole('alert')).toHaveTextContent('供应商拒绝')
  fireEvent.click(screen.getByRole('button', { name: '重试生成' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('retry', expect.objectContaining({ id: 'project' })))
  expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'plan' || action === 'submit')).toBe(false)
})

it('assembles a drama shot request with style, roles and product images', async () => {
  const brief = await assemble('drama')
  expect(screen.getByRole('heading', { name: '短剧带货视频' })).toBeVisible()
  expect(brief).toMatchObject({
    mode: 'drama',
    dramaStyle: 'romance',
    roleIds: ['r1'],
    roleImages: ['/roles/host.png'],
    images: ['/tmp/sku.png'],
    request: '一个反转故事',
  })
  cleanup()
})

it('keeps a drama draft when the page is left and reopened', () => {
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({
    drama: { brief: { ...dramaBrief(), request: '留下的故事', dramaStyle: 'romance' }, script: '## 镜头 1\n开场\n\n## 镜头 2\n收尾', step: 0, dirty: true },
  }))
  const page = render(<DramaPage />)
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('留下的故事')
  expect(screen.getByRole('button', { name: '甜宠恋爱' })).toHaveAttribute('aria-pressed', 'true')
  page.unmount()
  render(<DramaPage />)
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('留下的故事')
  fireEvent.click(screen.getByRole('tab', { name: /分镜剧本/ }))
  expect(screen.getByText(/开场/)).toBeVisible()
  expect(screen.getByText(/收尾/)).toBeVisible()
})

it('retries only the failed drama shot and keeps the submitted shot after a script edit', async () => {
  const shots = [
    { id: '1', prompt: '已提交的开场', status: 'succeeded', mediaTaskId: 'media-1', output: '/tmp/one.mp4' },
    { id: '2', prompt: '失败镜头', status: 'failed', mediaTaskId: 'media-2', error: '被拒绝' },
  ]
  task = { ...task, brief: dramaBrief(), script: '## 镜头 1\n已提交的开场\n\n## 镜头 2\n失败镜头', prompt: '已确认', approved: true, status: 'failed', shots }
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ drama: { brief: task.brief, task, script: task.script, step: 2, dirty: false } }))
  const original = vi.mocked(api.workbenchVideoTask).getMockImplementation()!
  vi.mocked(api.workbenchVideoTask).mockImplementation(async (action, input) => {
    if (action === 'save') {
      task = { ...task, script: String(input.script || ''), approved: false, shots }
    }
    if (action === 'retry') {
      task = {
        ...task,
        status: 'running',
        shots: [
          shots[0],
          { id: '2', prompt: '失败镜头', status: 'running' },
        ],
      }
    }
    return original(action, input)
  })
  render(<DramaPage />)
  expect(screen.getByRole('article', { name: '镜头 1' })).toHaveTextContent('已提交的开场')
  expect(screen.getByRole('article', { name: '镜头 2' })).toHaveTextContent('被拒绝')
  fireEvent.click(screen.getByRole('tab', { name: /分镜剧本/ }))
  fireEvent.click(screen.getByRole('button', { name: '编辑全文' }))
  fireEvent.change(screen.getByLabelText('视频方案'), { target: { value: '## 镜头 1\n改写开场\n\n## 镜头 2\n改写失败' } })
  fireEvent.click(screen.getByRole('tab', { name: /逐镜头生成/ }))
  fireEvent.click(screen.getByRole('button', { name: '重试失败镜头' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('retry', expect.objectContaining({ id: 'project' })))
  const save = vi.mocked(api.workbenchVideoTask).mock.calls.find(([action]) => action === 'save')
  expect(save?.[1].script).toContain('改写开场')
  expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'submit')).toBe(false)
  expect(screen.getByRole('article', { name: '镜头 1' })).toHaveTextContent('已提交的开场')
  expect(screen.getByRole('article', { name: '镜头 1' })).not.toHaveTextContent('改写开场')
})
