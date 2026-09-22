import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import { api, type Settings } from '../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../api/settingsCache'
import { makeProvider, makeSettings } from '../../settings/tabs/testFixtures'
import type { MediaTask } from '../../generated/mediaGeneration'
import VideoStudio from './VideoStudio'

vi.mock('@tauri-apps/api/core', async original => ({ ...await original<typeof import('@tauri-apps/api/core')>(), convertFileSrc: (path: string) => `asset://localhost${path}` }))
vi.mock('../../api/settingsCache', () => ({ getSettingsCached: vi.fn(), subscribeSettings: vi.fn() }))
let updateSettings: (settings: Settings) => void
const configured = () => makeSettings({
  providers: [makeProvider({ enabledModels: ['grok-imagine-video'] })],
  workbenchMedia: { imageModels: [], videoModels: [{ providerId: 'p1', model: 'grok-imagine-video' }] },
})
const task: MediaTask = { id: 'video-1', providerId: 'p1', model: 'grok-imagine-video', kind: 'video', status: 'succeeded', createdAt: '2026-09-22T01:00:00Z', remoteId: 'remote-1', canResume: false, error: null, outputs: [{ path: '/tmp/video.mp4', mime: 'video/mp4' }], origin: null, prompt: '' }
beforeEach(() => {
  vi.restoreAllMocks(); localStorage.clear()
  URL.revokeObjectURL = vi.fn()
  vi.mocked(getSettingsCached).mockResolvedValue(makeSettings())
  vi.mocked(subscribeSettings).mockImplementation(fn => { updateSettings = fn; return () => {} })
  vi.spyOn(api, 'legacyVideoOutputs').mockResolvedValue([])
  vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
  vi.spyOn(api, 'startMediaGeneration').mockResolvedValue(task)
  vi.spyOn(api, 'openLocalFile').mockResolvedValue(undefined)
})

it('preserves the studio layout and editable material area without a configured model', async () => {
  const { container } = render(<VideoStudio />)
  await screen.findByText('请先在「设置 → 媒体创作」中添加可用模型。')
  expect(container.querySelector('.video-studio .is-rail')).toBeInTheDocument()
  expect(container.querySelector('.vs-layout .vs-editor-column .vs-drop')).toBeInTheDocument()
  expect(container.querySelector('.vs-layout .vs-options')).toBeInTheDocument()
  expect(container.querySelector('.workbench-page')).toBeNull()
  expect(screen.getByText('把商品图片拖到这里')).toBeVisible()
  await userEvent.type(screen.getByLabelText('这次要拍什么'), '让背包转动')
  expect(screen.getByRole('button', { name: '开始生成' })).toBeDisabled()
  for (const label of ['拍摄方案', '视频拆解', '参考仿拍', '帮我设计视频']) expect(screen.queryByText(label)).toBeNull()

  act(() => updateSettings(configured()))
  await userEvent.click(screen.getByRole('button', { name: '视频模型' }))
  await userEvent.click(screen.getByRole('option', { name: 'OpenAI / grok-imagine-video' }))
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('让背包转动')
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
})

it('submits once through the shared API and shows playable results without a planning step', async () => {
  vi.mocked(getSettingsCached).mockResolvedValue(configured())
  localStorage.setItem('dsivio.workbench.media-choice.videoModels', JSON.stringify(['p1', 'grok-imagine-video']))
  let finish!: (task: MediaTask) => void
  vi.mocked(api.startMediaGeneration).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const { container } = render(<VideoStudio />)
  await userEvent.type(screen.getByLabelText('这次要拍什么'), '商品展示')
  const button = screen.getByRole('button', { name: '开始生成' })
  await waitFor(() => expect(button).toBeEnabled())
  await userEvent.dblClick(button)
  expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
  expect(api.startMediaGeneration).toHaveBeenCalledWith({ providerId: 'p1', model: 'grok-imagine-video', kind: 'video', prompt: '商品展示', images: [], options: {}, origin: null })
  vi.mocked(api.listMediaTasks).mockResolvedValue([task])
  await act(async () => finish(task))
  await waitFor(() => expect(screen.getByRole('tab', { name: '2 生成与成片' })).toHaveAttribute('aria-selected', 'true'))
  expect(container.querySelector('video')).toBeVisible()
  await userEvent.click(screen.getByRole('button', { name: '打开文件' }))
  expect(api.openLocalFile).toHaveBeenCalledWith('/tmp/video.mp4')
  await userEvent.click(screen.getByRole('tab', { name: '1 素材与要求' }))
  expect(screen.getByLabelText('这次要拍什么')).toHaveValue('商品展示')
})

it('accepts an image drop before choosing a model and keeps previous movies in the results view', async () => {
  vi.mocked(api.legacyVideoOutputs).mockResolvedValue([{ path: '/tmp/old.mp4', mime: 'video/mp4' }])
  const { container } = render(<VideoStudio />)
  fireEvent.drop(screen.getByLabelText('参考图片投放区'), { dataTransfer: { files: [new File(['png'], 'product.png', { type: 'image/png' })] } })
  expect(await screen.findByAltText('product.png')).toBeVisible()
  expect(screen.getByRole('button', { name: '开始生成' })).toBeDisabled()
  expect(screen.queryByText('历史作品')).not.toBeVisible()
  await userEvent.click(screen.getByRole('tab', { name: '2 生成与成片' }))
  expect(await screen.findByText('old.mp4')).toBeVisible()
  expect(container.querySelector('video')).toHaveAttribute('controls')
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
})
