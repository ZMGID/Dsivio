import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { SettingsSnapshot } from '../api/tauri'
import { makeSettings } from './tabs/testFixtures'
import { SettingsShell } from './SettingsShell'

const canonical: SettingsSnapshot = {
  settings: makeSettings({ settingsLanguage: 'zh', chat: { defaultLanguage: 'zh' } as never }),
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
      getAppVersion: async () => 'test',
      getDefaultPromptTemplates: async () => ({}),
      listSystemFonts: async () => [],
      getPermissionStatus: async () => ({ platform: 'windows', accessibility: true, screenRecording: true }),
      onHotkeyWarning: async () => () => {},
      onUpdateAvailable: async () => () => {},
      onReplaceTranslationPackProgress: async () => () => {},
      getLocalAsrStatus: async () => ({ operationId: null, state: 'notInstalled', installationId: null, serviceVersion: '0.2.0', model: 'small', languages: ['en', 'zh'], progress: null, error: null, runtime: { state: 'stopped', pid: null, activeTaskId: null } }),
      listMediaVoices: async () => [],
    },
  }
})

vi.mock('../api/im', () => ({
  getImStatus: vi.fn(),
  saveImCredentials: vi.fn(),
  clearImCredentials: vi.fn(),
  reconnectIm: vi.fn(),
  listImPairingRequests: vi.fn(),
  listImApprovedUsers: vi.fn(),
  approveImPairing: vi.fn(),
  denyImPairing: vi.fn(),
  revokeImUser: vi.fn(),
  beginImSetup: vi.fn(),
  pollImSetup: vi.fn(),
  cancelImSetup: vi.fn(),
  commitImSetup: vi.fn(),
  subscribeImStatus: vi.fn(async () => () => {}),
  subscribeImPairing: vi.fn(async () => () => {}),
}))

describe('Settings messaging page', () => {
  it('opens the messaging page from the nav and does not invent IM defaults', async () => {
    const user = userEvent.setup()
    render(
      <SettingsShell
        variant="embedded"
        initialTab="im"
        onClose={vi.fn()}
        onSettingsChange={vi.fn()}
        renderSessionCenter={() => null}
        renderPluginCenter={() => null}
        renderReleaseNotes={() => null}
      />,
    )
    expect(await screen.findByRole('button', { name: '即时通讯' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: '即时通讯' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('后端未返回 IM 配置')
    expect(screen.queryByLabelText('飞书 App ID')).toBeNull()
  })
})
