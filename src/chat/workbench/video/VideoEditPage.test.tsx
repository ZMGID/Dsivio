import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import { newVideoBrief, type VideoTask } from './projects/types'
import { VideoEditPage } from './VideoEditPage'

vi.mock('../../../api/tauri', () => ({ isTauriRuntime: () => true, api: {
  workbenchVideoBootstrap: vi.fn(), workbenchVideoTask: vi.fn(), workbenchVideoImage: vi.fn(), workbenchVideoPreview: vi.fn(), workbenchVideoPoster: vi.fn(), studioTaskLibrary: vi.fn(async () => ({})),
} }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ revision: 0, value: null })) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(async () => ({ providers: [], workbenchMedia: { imageModels: [], videoModels: [] } })), subscribeSettings: vi.fn(() => () => {}) }))
vi.mock('../../api', () => ({ chatApi: { getAssistants: vi.fn(async () => []) } }))

const PLAN = JSON.stringify({
  clips: [{ source: '/tmp/a.mp4', start: 0, end: 2 }, { source: '/tmp/b.mp4' }],
  aspect: '9:16',
  fit: 'pad',
}, null, 2)
let task: VideoTask
let releasePlan: ((value: VideoTask) => void) | undefined

beforeEach(() => {
  vi.clearAllMocks()
  localStorage.clear()
  releasePlan = undefined
  task = { id: 'project', revision: 1, updatedAt: 1, brief: { ...newVideoBrief('editing'), request: '' }, script: '', prompt: '', approved: false, status: 'draft' }
  vi.mocked(api.workbenchVideoBootstrap).mockResolvedValue({ tasks: [], templates: [], config: {}, root: '', configPath: '', dependencies: {} } as never)
  vi.mocked(api.workbenchVideoTask).mockImplementation(async (action, input) => {
    if (action === 'create') task = { ...task, brief: input.brief as VideoTask['brief'] }
    if (action === 'save') task = { ...task, brief: input.brief as VideoTask['brief'], script: String(input.script || ''), approved: false, prompt: '' }
    if (action === 'plan') {
      task = { ...task, script: PLAN, status: 'draft' }
      if (releasePlan) {
        const pending = task
        return new Promise(resolve => { releasePlan = () => resolve({ ...pending }) })
      }
    }
    if (action === 'approve') task = { ...task, approved: true, status: 'approved', prompt: task.script }
    if (action === 'prepare' || action === 'prompt_result') task = { ...task, prompt: task.script, approved: true }
    if (action === 'submit' || action === 'retry') task = { ...task, status: 'running', mediaTaskId: 'media-1' }
    if (action === 'cancel') task = { ...task, status: 'cancelled' }
    if (action !== 'get') task = { ...task, revision: task.revision + 1 }
    return { ...task }
  })
  vi.mocked(api.workbenchVideoPreview).mockResolvedValue('data:video/mp4;base64,bW92aWU=')
  vi.mocked(api.workbenchVideoPoster).mockRejectedValue(new Error('no poster'))
  vi.mocked(open).mockResolvedValue(['/tmp/a.mp4', '/tmp/b.mp4'])
})

async function planClips() {
  render(<VideoEditPage />)
  fireEvent.click(screen.getByRole('button', { name: '添加视频片段' }))
  expect(await screen.findByText('a.mp4')).toBeVisible()
  fireEvent.change(screen.getByLabelText('剪辑要求'), { target: { value: '竖屏拼接，第一段留到 4 秒' } })
  fireEvent.click(screen.getByRole('button', { name: '生成剪辑方案' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('plan', expect.anything()))
}

async function openPlannedEditor() {
  await planClips()
  expect(await screen.findByLabelText('a.mp4 出点')).toHaveValue(2)
}

it('assembles clip paths and a brief, then submits the edited plan locally', async () => {
  await openPlannedEditor()
  const saved = vi.mocked(api.workbenchVideoTask).mock.calls.filter(([action]) => action === 'save' || action === 'create').at(-1)
  expect(saved?.[1].brief).toMatchObject({
    mode: 'editing',
    clips: ['/tmp/a.mp4', '/tmp/b.mp4'],
    request: '竖屏拼接，第一段留到 4 秒',
  })
  expect(screen.queryByText('请选择工作台视频模型')).toBeNull()
  fireEvent.change(screen.getByLabelText('a.mp4 出点'), { target: { value: '4' } })
  fireEvent.click(screen.getByRole('tab', { name: /本地成片/ }))
  fireEvent.click(screen.getByRole('button', { name: '开始本地剪辑' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('submit', expect.anything()))
  const script = vi.mocked(api.workbenchVideoTask).mock.calls.filter(([action]) => action === 'save').at(-1)?.[1].script as string
  expect(script).toContain('"end": 4')
  expect(script).toContain('/tmp/b.mp4')
  expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'quote')).toBe(false)
})

it('keeps an edit draft when the page is left and reopened', () => {
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({
    editing: { brief: { ...newVideoBrief('editing'), request: '留下的剪辑', clips: ['/tmp/a.mp4'] }, script: PLAN, step: 1, dirty: true },
  }))
  const page = render(<VideoEditPage />)
  expect(screen.getByRole('heading', { name: '产品视频编辑' })).toBeVisible()
  fireEvent.click(screen.getByRole('tab', { name: /剪辑方案/ }))
  expect(screen.getByLabelText('a.mp4 出点')).toHaveValue(2)
  page.unmount()
  render(<VideoEditPage />)
  fireEvent.click(screen.getByRole('tab', { name: /剪辑方案/ }))
  expect(screen.getByLabelText('a.mp4 出点')).toHaveValue(2)
  expect(api.workbenchVideoTask).not.toHaveBeenCalled()
})

it('retries a failed local edit and cancels a running one without planning again', async () => {
  const failed = { ...task, brief: { ...newVideoBrief('editing'), request: '重做', clips: ['/tmp/a.mp4'] }, script: PLAN, prompt: PLAN, approved: true, status: 'failed', error: '素材缺失', remote: { route: '' as const, id: '', base_url: '' } }
  task = failed
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ editing: { brief: failed.brief, task: failed, script: PLAN, step: 2, dirty: false } }))
  const page = render(<VideoEditPage />)
  expect(await screen.findByRole('alert')).toHaveTextContent('素材缺失')
  fireEvent.click(screen.getByRole('button', { name: '重试剪辑' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('retry', expect.objectContaining({ id: 'project' })))
  expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'plan' || action === 'quote')).toBe(false)
  page.unmount()
  localStorage.clear()
  const running = { ...failed, status: 'running', error: undefined, mediaTaskId: 'media-1', remote: { route: '' as const, id: '', base_url: '' } }
  task = running
  localStorage.setItem('dsivio-video-drafts-v1', JSON.stringify({ editing: { brief: running.brief, task: running, script: PLAN, step: 2, dirty: false } }))
  render(<VideoEditPage />)
  fireEvent.click(screen.getByRole('button', { name: '取消剪辑' }))
  await waitFor(() => expect(api.workbenchVideoTask).toHaveBeenCalledWith('cancel', expect.objectContaining({ id: 'project' })))
  expect(vi.mocked(api.workbenchVideoTask).mock.calls.some(([action]) => action === 'submit')).toBe(false)
})

it('ignores a late edit plan after starting a new task', async () => {
  releasePlan = () => {}
  await planClips()
  fireEvent.click(screen.getByRole('button', { name: '新建' }))
  await act(async () => { releasePlan?.({ ...task, script: PLAN, revision: 9 }) })
  await waitFor(() => expect(screen.getByLabelText('剪辑要求')).toHaveValue(''))
  expect(screen.queryByLabelText('a.mp4 出点')).toBeNull()
})
