import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { open, save } from '@tauri-apps/plugin-dialog'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { SubtitlePage } from './SubtitlePage'

vi.mock('../../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    startMediaGeneration: vi.fn(),
    listMediaTasks: vi.fn(async () => []),
    getMediaTask: vi.fn(),
    cancelMediaTask: vi.fn(),
    openLocalFile: vi.fn(async () => undefined),
    exportSubtitleFile: vi.fn(async () => '/tmp/out.srt'),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(), save: vi.fn() }))

const task = {
  id: 'sub-1',
  providerId: 'local',
  model: 'ffmpeg-subtitle',
  kind: 'edit',
  status: 'succeeded',
  createdAt: '2026-10-02T00:00:00Z',
  error: null,
  remoteId: null,
  outputs: [{ path: '/tmp/subtitles.srt', mime: 'application/x-subrip' }],
  canResume: false,
  origin: 'workbench/subs',
  prompt: '',
  result: { language: 'zh', segments: [] },
  requestHash: null,
  cancellation: null,
} as MediaTask

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  vi.mocked(api.startMediaGeneration).mockResolvedValue({ ...task, status: 'running', outputs: [] })
  vi.mocked(open).mockResolvedValue('/tmp/demo.mp4')
  vi.mocked(save).mockResolvedValue('/tmp/exported.srt')
})

async function chooseAndSubmit() {
  render(<SubtitlePage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalled())
  fireEvent.click(screen.getByRole('button', { name: '选择视频' }))
  await screen.findByText('/tmp/demo.mp4')
  fireEvent.click(screen.getByRole('checkbox', { name: '烧录到视频' }))
  fireEvent.click(screen.getByRole('button', { name: '生成字幕' }))
}

it('submits a local ffmpeg subtitle edit and keeps the task after leaving', async () => {
  await chooseAndSubmit()
  await waitFor(() => expect(api.startMediaGeneration).toHaveBeenCalledWith({
    providerId: 'local',
    model: 'ffmpeg-subtitle',
    kind: 'edit',
    prompt: '',
    images: [],
    options: { video: '/tmp/demo.mp4', language: 'zh', burn: true },
    origin: 'workbench/subs',
  }))
  expect(api.listMediaTasks).toHaveBeenCalledWith({ origin: 'workbench/subs' })
  expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
})

it('shows a subtitle failure and submits again on retry', async () => {
  vi.mocked(api.startMediaGeneration).mockRejectedValueOnce(new Error('找不到视频'))
  await chooseAndSubmit()
  expect(await screen.findByText(/找不到视频/)).toBeVisible()
  expect(screen.queryByText('完成')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '生成字幕' }))
  await waitFor(() => expect(api.startMediaGeneration).toHaveBeenCalledTimes(2))
})

it('shows the same validation error again after the first toast expired', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  try {
    render(<SubtitlePage />)
    await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalled())
    fireEvent.click(screen.getByRole('button', { name: '生成字幕' }))
    expect(await screen.findByRole('alert')).toBeVisible()
    act(() => { vi.advanceTimersByTime(5000) })
    expect(screen.queryByRole('alert')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '生成字幕' }))
    expect(await screen.findByRole('alert')).toBeVisible()
    expect(api.startMediaGeneration).not.toHaveBeenCalled()
  } finally {
    vi.useRealTimers()
  }
})

it('drops a subtitle result that arrives after the page is left', async () => {
  let finish: (value: MediaTask) => void = () => {}
  vi.mocked(api.startMediaGeneration).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const page = render(<SubtitlePage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalled())
  fireEvent.click(screen.getByRole('button', { name: '选择视频' }))
  await screen.findByText('/tmp/demo.mp4')
  fireEvent.click(screen.getByRole('button', { name: '生成字幕' }))
  await waitFor(() => expect(api.startMediaGeneration).toHaveBeenCalledTimes(1))
  page.unmount()
  await act(async () => { finish({ ...task, prompt: 'late-subtitle' }) })
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  render(<SubtitlePage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalledTimes(2))
  expect(screen.queryByText('late-subtitle')).toBeNull()
})

it('reopens the subtitle list and exports the srt', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([task])
  const page = render(<SubtitlePage />)
  expect(await screen.findByRole('button', { name: '打开文件' })).toBeVisible()
  page.unmount()
  render(<SubtitlePage />)
  fireEvent.click(await screen.findByRole('button', { name: '导出字幕' }))
  await waitFor(() => expect(api.exportSubtitleFile).toHaveBeenCalledWith('/tmp/subtitles.srt', '/tmp/exported.srt'))
  fireEvent.click(screen.getByRole('button', { name: '打开文件' }))
  expect(api.openLocalFile).toHaveBeenCalledWith('/tmp/subtitles.srt')
})
