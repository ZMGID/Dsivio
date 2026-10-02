import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { confirmDialog } from '../../components/dialogQueue'
import { DsvideoProjects } from './DsvideoProjects'
import { dsvideoProjectsApi } from '../../api/dsvideoProjects'
vi.mock('../../components/dialogQueue', () => ({ confirmDialog: vi.fn() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../../api/dsvideoProjects', () => ({ dsvideoProjectsApi: { list: vi.fn().mockResolvedValue({ version: 1, current: null, projects: [], document: '/app/PROJECTS.md' }), init: vi.fn(), bind: vi.fn(), remove: vi.fn() } }))
afterEach(cleanup)
beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(dsvideoProjectsApi.list).mockResolvedValue({ version: 1, current: null, projects: [], document: '/app/PROJECTS.md' })
})
it('首次使用登记指定目录，失败保留选择，成功后绑定对话项目', async () => {
  const onChoose = vi.fn()
  const project = { id: 'project-1', name: '新视频', rootPath: '/videos/new' }
  vi.mocked(dsvideoProjectsApi.init).mockRejectedValueOnce(new Error('目录不可写')).mockResolvedValue({ version: 1, current: 'project-1', projects: [], document: '/app/PROJECTS.md' })
  vi.mocked(dsvideoProjectsApi.bind).mockResolvedValue(project)
  render(<DsvideoProjects lang="zh" onChoose={onChoose} onClose={vi.fn()} />)
  await screen.findByText('还没有 Dsvideo 项目，请选择或创建一个目录。')
  fireEvent.change(screen.getByRole('textbox', { name: '项目目录' }), { target: { value: '/videos/new' } })
  fireEvent.change(screen.getByRole('textbox', { name: '项目名称' }), { target: { value: '新视频' } })
  fireEvent.click(screen.getByRole('button', { name: '初始化并使用' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('目录不可写')
  expect(onChoose).not.toHaveBeenCalled()
  expect(dsvideoProjectsApi.bind).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '初始化并使用' }))
  await waitFor(() => expect(onChoose).toHaveBeenCalledWith(project))
  expect(dsvideoProjectsApi.init).toHaveBeenLastCalledWith('/videos/new', '新视频')
  expect(dsvideoProjectsApi.bind).toHaveBeenCalledWith('/videos/new')
})
it('离开项目选择页后，迟到的准备结果不启动对话', async () => {
  vi.mocked(dsvideoProjectsApi.bind).mockClear()
  let finish!: (value: { id: string; name: string; rootPath: string }) => void
  vi.mocked(dsvideoProjectsApi.bind).mockReturnValue(new Promise(resolve => { finish = resolve }))
  const onChoose = vi.fn()
  const view = render(<DsvideoProjects lang="zh" onChoose={onChoose} onClose={vi.fn()} />)
  await screen.findByText('还没有 Dsvideo 项目，请选择或创建一个目录。')
  fireEvent.change(screen.getByRole('textbox', { name: '项目目录' }), { target: { value: '/videos/new' } })
  fireEvent.click(screen.getByRole('button', { name: '初始化并使用' }))
  await waitFor(() => expect(dsvideoProjectsApi.bind).toHaveBeenCalled())
  view.unmount()
  finish({ id: 'project-1', name: '新视频', rootPath: '/videos/new' })
  await Promise.resolve()
  expect(onChoose).not.toHaveBeenCalled()
})

it('已有项目优先继续，新建表单按需展开，隐藏登记文档路径', async () => {
  vi.mocked(dsvideoProjectsApi.list).mockResolvedValue({ version: 1, current: 'video-1', document: '/app/PROJECTS.md', projects: [
    { id: 'video-1', name: 'Tes', path: '/videos/Tes', available: true, initializedAt: 1, lastUsedAt: 1 },
  ] })
  const project = { id: 'project-1', name: 'Dsvideo · Tes', rootPath: '/videos/Tes' }
  vi.mocked(dsvideoProjectsApi.bind).mockResolvedValue(project)
  const onChoose = vi.fn()
  render(<DsvideoProjects lang="zh" onChoose={onChoose} onClose={vi.fn()} />)
  await screen.findByText('Tes（上次使用）')
  expect(screen.queryByRole('textbox', { name: '项目目录' })).toBeNull()
  expect(screen.queryByText(/PROJECTS.md|项目登记文档/)).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '新建项目' }))
  expect(screen.getByRole('textbox', { name: '项目目录' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '返回已有项目' }))
  expect(screen.queryByRole('textbox', { name: '项目目录' })).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '继续项目' }))
  await waitFor(() => expect(onChoose).toHaveBeenCalledWith(project))
  expect(dsvideoProjectsApi.init).not.toHaveBeenCalled()
  expect(dsvideoProjectsApi.bind).toHaveBeenCalledWith('/videos/Tes')
})

it('删除登记需确认，失败保留项目，成功后回到首次创建界面', async () => {
  vi.mocked(dsvideoProjectsApi.list).mockResolvedValue({ version: 1, current: 'v1', document: '/app/PROJECTS.md', projects: [
    { id: 'v1', name: 'Tes', path: '/videos/Tes', available: false, initializedAt: 1, lastUsedAt: 1 },
  ] })
  vi.mocked(confirmDialog).mockResolvedValueOnce(false).mockResolvedValue(true)
  vi.mocked(dsvideoProjectsApi.remove).mockRejectedValueOnce(new Error('保存失败')).mockResolvedValue({ version: 1, current: null, document: '/app/PROJECTS.md', projects: [] })
  render(<DsvideoProjects lang="zh" onChoose={vi.fn()} onClose={vi.fn()} />)
  await screen.findByText('Tes（上次使用）')
  fireEvent.click(screen.getByRole('button', { name: '删除项目 Tes' }))
  await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1))
  await waitFor(() => expect(screen.getByRole('button', { name: '删除项目 Tes' })).not.toBeDisabled())
  expect(dsvideoProjectsApi.remove).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '删除项目 Tes' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('保存失败')
  expect(screen.getByText('Tes（上次使用）')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '删除项目 Tes' }))
  await screen.findByText('还没有 Dsvideo 项目，请选择或创建一个目录。')
  expect(screen.queryByText('Tes（上次使用）')).toBeNull()
  expect(dsvideoProjectsApi.remove).toHaveBeenLastCalledWith('/videos/Tes')
})
