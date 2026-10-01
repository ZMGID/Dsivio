import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { DsvideoProjects } from './DsvideoProjects'
import { dsvideoProjectsApi } from '../../api/dsvideoProjects'
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../../api/dsvideoProjects', () => ({ dsvideoProjectsApi: { list: vi.fn().mockResolvedValue({ version: 1, current: null, projects: [], document: '/app/PROJECTS.md' }), init: vi.fn(), bind: vi.fn() } }))
afterEach(cleanup)
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
