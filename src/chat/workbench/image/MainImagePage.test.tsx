import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../../api/settingsCache'
import { makeProvider, makeSettings } from '../../../settings/tabs/testFixtures'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { MainImagePage } from './MainImagePage'
import { buildMainImagePrompt } from './mainImagePrompt'
import { i18n } from '../../../components/i18n'

const zh = i18n.zh

vi.mock('@tauri-apps/api/core', async (original) => ({ ...await original<typeof import('@tauri-apps/api/core')>(), convertFileSrc: (path: string) => `asset://localhost${path}` }))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(), subscribeSettings: vi.fn() }))

const provider = makeProvider({ enabledModels: ['gpt-image-1'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } } } })
const configured = () => makeSettings({ providers: [provider], workbenchMedia: { imageModels: [{ providerId: provider.id, model: 'gpt-image-1' }], videoModels: [] } })
const done: MediaTask = { id: 'img-1', providerId: provider.id, model: 'gpt-image-1', kind: 'image', status: 'succeeded', createdAt: '2026-09-22T01:00:00Z', remoteId: null, canResume: false, error: null, outputs: [{ path: '/tmp/main-0.png', mime: 'image/png' }], origin: 'workbench/main', prompt: '白底' }

beforeEach(() => {
  vi.restoreAllMocks(); localStorage.clear()
  URL.createObjectURL = vi.fn(() => 'blob:product')
  URL.revokeObjectURL = vi.fn()
  vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['png'], { type: 'image/png' }) })))
  vi.mocked(getSettingsCached).mockResolvedValue(configured())
  vi.mocked(subscribeSettings).mockImplementation(() => () => {})
  vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
  vi.spyOn(api, 'startMediaGeneration').mockResolvedValue(done)
})

it('lists history by page origin, not by the selected model', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([done])
  render(<MainImagePage />)
  expect(await screen.findByText('完成')).toBeVisible()
  expect(api.listMediaTasks).toHaveBeenCalledWith({ origin: 'workbench/main' })
  expect(screen.getByRole('img', { name: zh.workbenchMainTitle })).toHaveAttribute('src', 'asset://localhost/tmp/main-0.png')
})

it('refuses to submit without a model or product image, then submits one request tagged with the page origin', async () => {
  render(<MainImagePage />)
  await screen.findByRole('button', { name: '图片模型' })
  await userEvent.click(screen.getByRole('button', { name: '开始生成主图' }))
  expect(screen.getByText('请先选择本次使用的图片模型')).toBeVisible()
  expect(api.startMediaGeneration).not.toHaveBeenCalled()

  await userEvent.click(screen.getByRole('button', { name: '图片模型' }))
  await userEvent.click(screen.getByRole('option', { name: 'OpenAI / gpt-image-1' }))
  await userEvent.click(screen.getByRole('button', { name: '开始生成主图' }))
  expect(screen.getByText('请先上传产品图')).toBeVisible()

  const input = document.querySelector<HTMLInputElement>('input[type="file"]')!
  await userEvent.upload(input, new File(['png'], 'product.png', { type: 'image/png' }))
  await userEvent.type(screen.getByPlaceholderText(zh.workbenchMainBriefHint), '不锈钢保温杯')
  vi.mocked(api.listMediaTasks).mockResolvedValue([done])
  await userEvent.click(screen.getByRole('button', { name: '开始生成主图' }))
  await waitFor(() => expect(api.startMediaGeneration).toHaveBeenCalledTimes(1))
  expect(api.startMediaGeneration).toHaveBeenCalledWith({
    providerId: provider.id,
    model: 'gpt-image-1',
    kind: 'image',
    prompt: buildMainImagePrompt({ brief: '不锈钢保温杯', style: 'white', ratio: '1:1' }, zh),
    images: [expect.stringMatching(/^data:image\/png;base64,/)],
    options: { aspect_ratio: '1:1', size: '2K', n: 4 },
    origin: 'workbench/main',
  })
  expect(await screen.findByText('完成')).toBeVisible()
})
