import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { AssetLibraryPage } from './AssetLibraryPage'
import { AssetPicker } from './AssetPicker'

vi.mock('../../../api/tauri', () => ({
  api: {
    listMediaTasks: vi.fn(),
    deleteMediaTask: vi.fn(),
    importMediaArtifact: vi.fn(),
    openLocalFile: vi.fn(),
    revealGeneratedFile: vi.fn(),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))

function task(partial: Partial<MediaTask>): MediaTask {
  return {
    id: 'task-1',
    providerId: 'local',
    model: 'import',
    kind: 'image',
    status: 'succeeded',
    createdAt: '2026-10-02T00:00:00Z',
    error: null,
    remoteId: null,
    outputs: [{ path: '/tmp/poster.png', mime: 'image/png' }],
    canResume: false,
    origin: 'workbench/main',
    prompt: '白底',
    result: { title: '海报' },
    requestHash: null,
    cancellation: null,
    ...partial,
  }
}

beforeEach(() => {
  vi.mocked(api.listMediaTasks).mockReset()
  vi.mocked(api.deleteMediaTask).mockReset()
  vi.mocked(api.importMediaArtifact).mockReset()
  vi.mocked(api.openLocalFile).mockReset()
  vi.mocked(api.revealGeneratedFile).mockReset()
  vi.mocked(open).mockReset()
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, status: 200, text: async () => '# 通勤包\n正文' })))
})

it('filters completed outputs by kind and keyword, and drops a late list', async () => {
  const poster = task({ id: 'img', result: { title: '海报' } })
  const clip = task({ id: 'vid', kind: 'video', result: { title: '口播' }, outputs: [{ path: '/tmp/clip.mp4', mime: 'video/mp4' }] })
  let resolveFirst: (value: MediaTask[]) => void = () => {}
  vi.mocked(api.listMediaTasks).mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve }))
  render(<AssetLibraryPage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalledTimes(1))
  vi.mocked(api.listMediaTasks).mockResolvedValueOnce([poster, clip])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByText('海报')).toBeInTheDocument()
  expect(screen.getByText('口播')).toBeInTheDocument()
  await act(async () => { resolveFirst([task({ id: 'old', result: { title: '旧海报' } })]) })
  expect(screen.queryByText('旧海报')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: /^视频/ }))
  expect(screen.queryByText('海报')).toBeNull()
  expect(screen.getByText('口播')).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: /^全部/ }))
  fireEvent.change(screen.getByRole('searchbox', { name: '搜索标题、提示词或路径' }), { target: { value: '口播' } })
  expect(screen.queryByText('海报')).toBeNull()
  expect(screen.getByText('口播')).toBeInTheDocument()
})

it('shows the refusal when delete is rejected and removes the file when delete succeeds', async () => {
  const poster = task({})
  vi.mocked(api.listMediaTasks).mockResolvedValue([poster])
  vi.mocked(api.deleteMediaTask).mockRejectedValueOnce(new Error('任务仍在运行，不能删除'))
  render(<AssetLibraryPage />)
  fireEvent.click(await screen.findByRole('button', { name: '删除 海报' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('任务仍在运行，不能删除')
  expect(screen.getByText('海报')).toBeInTheDocument()
  vi.mocked(api.deleteMediaTask).mockResolvedValueOnce()
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  fireEvent.click(screen.getByRole('button', { name: '删除 海报' }))
  await waitFor(() => expect(api.deleteMediaTask).toHaveBeenCalledWith('task-1'))
  await waitFor(() => expect(screen.queryByText('海报')).toBeNull())
})

it('imports a local file through the media record and reports a rejected import', async () => {
  vi.mocked(open).mockResolvedValueOnce('/Users/me/photo.png')
  vi.mocked(api.importMediaArtifact).mockResolvedValueOnce(task({ id: 'imported', result: { title: 'photo' } }))
  render(<AssetLibraryPage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalled())
  vi.mocked(api.listMediaTasks).mockResolvedValue([task({ id: 'imported', result: { title: 'photo' }, origin: 'workbench/assets' })])
  fireEvent.click(screen.getByRole('button', { name: '导入本机文件' }))
  await waitFor(() => expect(api.importMediaArtifact).toHaveBeenCalledWith('workbench/assets', '/Users/me/photo.png', 'photo'))
  expect(await screen.findByText('photo')).toBeInTheDocument()

  vi.mocked(open).mockResolvedValueOnce('/Users/me/payload.exe')
  vi.mocked(api.importMediaArtifact).mockRejectedValueOnce(new Error('不支持导入这种文件：exe'))
  fireEvent.click(screen.getByRole('button', { name: '导入本机文件' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('不支持导入这种文件：exe')
})

it('opens and reveals the output file', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([task({})])
  render(<AssetLibraryPage />)
  fireEvent.click(await screen.findByRole('button', { name: '打开 /tmp/poster.png' }))
  await waitFor(() => expect(api.openLocalFile).toHaveBeenCalledWith('/tmp/poster.png'))
  fireEvent.click(screen.getByRole('button', { name: '在文件夹中显示 /tmp/poster.png' }))
  await waitFor(() => expect(api.revealGeneratedFile).toHaveBeenCalledWith('/tmp/poster.png'))
})

it('loads the library again after leaving and reopening', async () => {
  const view = render(<AssetLibraryPage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalledTimes(1))
  view.unmount()
  vi.mocked(api.listMediaTasks).mockResolvedValue([task({})])
  render(<AssetLibraryPage />)
  expect(await screen.findByText('海报')).toBeInTheDocument()
  expect(api.listMediaTasks).toHaveBeenCalledTimes(2)
})

it('returns image paths and text bodies from the picker, and keeps a failed read', async () => {
  const image = task({ id: 'img', kind: 'image', result: { title: '主图' }, outputs: [{ path: '/tmp/main.png', mime: 'image/png' }] })
  const note = task({
    id: 'note',
    kind: 'text',
    result: { title: '通勤包' },
    prompt: '帆布',
    outputs: [{ path: '/tmp/output.md', mime: 'text/markdown' }],
  })
  const clip = task({ id: 'vid', kind: 'video', result: { title: '口播' }, outputs: [{ path: '/tmp/clip.mp4', mime: 'video/mp4' }] })
  vi.mocked(api.listMediaTasks).mockResolvedValue([image, note, clip])
  const onPick = vi.fn()
  const onClose = vi.fn()
  render(<AssetPicker accept={['image', 'text']} multiple onPick={onPick} onClose={onClose} />)
  expect(await screen.findByRole('button', { name: /主图/ })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: /口播/ })).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: /主图/ }))
  fireEvent.click(screen.getByRole('button', { name: /通勤包/ }))
  fireEvent.click(screen.getByRole('button', { name: '使用所选' }))
  await waitFor(() => expect(onPick).toHaveBeenCalledWith([
    expect.objectContaining({ id: 'img', kind: 'image', path: '/tmp/main.png', text: null }),
    expect.objectContaining({ id: 'note', kind: 'text', path: '/tmp/output.md', text: '# 通勤包\n正文' }),
  ]))

  onPick.mockClear()
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: false, status: 404, text: async () => '' })))
  fireEvent.click(screen.getByRole('button', { name: '使用所选' }))
  expect(await screen.findByRole('alert')).toHaveTextContent('无法读取文案（404）')
  expect(onPick).not.toHaveBeenCalled()
})
