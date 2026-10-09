import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api, type Settings } from '../api/tauri'
import { getSettingsCached, getSettingsSnapshotCached, importSettingsSnapshotCached, saveSettingsSnapshotCached } from '../api/settingsCache'
import { makeProvider, makeSettings } from '../settings/tabs/testFixtures'
import { chatApi } from './api'
import Chat from './Chat'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@xterm/xterm', () => ({ Terminal: class {} }))
vi.mock('@xterm/addon-webgl', () => ({ WebglAddon: class {} }))
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }) }))
vi.mock('../api/settingsCache', async original => ({
  ...await original<typeof import('../api/settingsCache')>(),
  getSettingsCached: vi.fn(),
  getSettingsSnapshotCached: vi.fn(),
  importSettingsSnapshotCached: vi.fn(),
  saveSettingsSnapshotCached: vi.fn(),
}))

afterEach(() => { vi.restoreAllMocks(); window.localStorage.clear() })

it.each(['manual', 'company'] as const)('refreshes the chat draft from %s setup before the first conversation', async route => {
  // Settings reads/writes and native event transport are external seams; the wizard,
  // completion handler, chat draft and model selector remain real.
  for (const key of Object.keys(api) as Array<keyof typeof api>) {
    if (key.startsWith('on') && typeof api[key] === 'function') vi.spyOn(api, key).mockResolvedValue(() => {})
  }
  vi.spyOn(api, 'chatSyncState').mockResolvedValue(undefined)
  vi.spyOn(api, 'scheduledTasksList').mockResolvedValue([])
  vi.spyOn(chatApi, 'getConversations').mockResolvedValue([])
  vi.spyOn(chatApi, 'getProjects').mockResolvedValue([])
  vi.spyOn(chatApi, 'getSets').mockResolvedValue([])
  vi.spyOn(chatApi, 'getAssistants').mockResolvedValue([])
  vi.spyOn(chatApi, 'getConversationPins').mockResolvedValue({})
  const initial = makeSettings({ settingsLanguage: 'zh', chat: {} as Settings['chat'] })
  let cached = initial
  const binding = { providerId: 'p1', model: 'configured-chat-model' }
  const configured = {
    ...initial,
    providers: [makeProvider({ enabledModels: [binding.model], availableModels: [binding.model] })],
    translatorProviderId: binding.providerId, translatorModel: binding.model,
    chatProviderId: '', chatModel: '',
    lens: { ...initial.lens, ...binding },
    screenshotTranslation: { ...initial.screenshotTranslation, ...binding },
  }
  vi.mocked(getSettingsCached).mockImplementation(async () => cached)
  vi.mocked(getSettingsSnapshotCached).mockResolvedValue({ settings: configured, version: { epoch: 'test', revision: 1 } })
  vi.mocked(saveSettingsSnapshotCached).mockImplementation(async settings => {
    cached = settings
    return { settings, version: { epoch: 'test', revision: 2 } }
  })
  window.history.replaceState(null, '', '#chat/onboarding')
  render(<Chat onSettingsChange={() => {}} />)
  if (route === 'manual') {
    fireEvent.click(await screen.findByRole('button', { name: '手动配置' }))
    for (let step = 0; step < 3; step++) fireEvent.click(screen.getByRole('button', { name: '下一步' }))
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: '开始使用 Dsivio' })) })
  } else {
    vi.mocked(open).mockResolvedValue('/company/settings.json')
    vi.mocked(importSettingsSnapshotCached).mockImplementation(async () => {
      cached = { ...configured, onboardingStatus: 'completed' }
      return { settings: cached, version: { epoch: 'test', revision: 2 } }
    })
    const importButton = await screen.findByRole('button', { name: '导入公司配置' })
    await act(async () => { fireEvent.click(importButton) })
    expect(importSettingsSnapshotCached).toHaveBeenCalledWith('/company/settings.json', { epoch: 'test', revision: 1 }, true)
  }
  await waitFor(() => expect(screen.getByRole('button', { name: binding.model })).toBeInTheDocument())
  expect(screen.queryByRole('button', { name: 'GPT-4o' })).not.toBeInTheDocument()
  expect(cached.onboardingStatus).toBe('completed')
})
