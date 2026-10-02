import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { SettingsSnapshot } from '../api/tauri'
import { makeProvider, makeSettings } from './tabs/testFixtures'
import { SettingsShell } from './SettingsShell'

const canonical: SettingsSnapshot = {
  settings: makeSettings({
    settingsLanguage: 'zh',
    chat: { defaultLanguage: 'zh' } as never,
    providers: [makeProvider({ enabledModels: ['gpt-image-1'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } } } })],
    workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'gpt-image-1' }] },
  }),
  version: { epoch: 'test', revision: 1 },
}

vi.mock('../api/settingsCache', () => ({
  peekSettingsSnapshot: () => canonical,
  getSettingsSnapshotCached: async () => canonical,
  refreshSettingsSnapshot: async () => canonical,
  saveSettingsSnapshotCached: () => new Promise(() => {}),
  subscribeSettingsSnapshot: () => () => {},
  importSettingsSnapshotCached: async () => canonical,
  updateSettingsCached: vi.fn(),
}))
vi.mock('../api/tauri', async (importOriginal) => {
  const original = await importOriginal<typeof import('../api/tauri')>()
  return {
    ...original,
    api: {
      ...original.api,
      getAppVersion: async () => 'test', getDefaultPromptTemplates: async () => ({}), listSystemFonts: async () => [],
      getPermissionStatus: async () => ({ platform: 'windows', accessibility: true, screenRecording: true }),
      onHotkeyWarning: async () => () => {}, onUpdateAvailable: async () => () => {}, onReplaceTranslationPackProgress: async () => () => {},
      getLocalAsrStatus: async () => ({ operationId: null, state: 'notInstalled', installationId: null, serviceVersion: '0.2.0', model: 'small', languages: ['en', 'zh'], progress: null, error: null, runtime: { state: 'stopped', pid: null, activeTaskId: null } }),
      listMediaVoices: async () => [],
    },
  }
})

describe('Media creation page header', () => {
  it('switches the media type from the header, like the plugin center, and shows each type\'s pool', async () => {
    const user = userEvent.setup()
    render(<SettingsShell variant="embedded" initialTab="media" onClose={vi.fn()} onSettingsChange={vi.fn()}
      renderSessionCenter={() => null} renderPluginCenter={() => null} renderReleaseNotes={() => null} />)
    const nav = await screen.findByRole('navigation', { name: '媒体类型' })
    const types = within(nav).getAllByRole('button')
    expect(types.map(button => button.textContent?.replace(/\d+$/, ''))).toEqual(['图片', '视频', '语音', '转写'])
    expect(within(nav).getByRole('button', { name: /图片/ })).toHaveAttribute('aria-current', 'page')
    expect(screen.getByRole('region', { name: '图片模型池' })).toBeInTheDocument()
    await user.click(within(nav).getByRole('button', { name: /转写/ }))
    expect(within(nav).getByRole('button', { name: /转写/ })).toHaveAttribute('aria-current', 'page')
    expect(screen.queryByRole('region', { name: '图片模型池' })).toBeNull()
    expect(screen.getByRole('region', { name: '转写模型池' })).toBeInTheDocument()
    expect(await screen.findByText(/本地转写 · WhisperX/)).toBeInTheDocument()
  })
})
