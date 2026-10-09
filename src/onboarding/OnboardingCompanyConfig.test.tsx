import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { getSettingsSnapshotCached, importSettingsSnapshotCached, refreshSettingsSnapshot, saveSettingsSnapshotCached } from '../api/settingsCache'
import { makeSettings } from '../settings/tabs/testFixtures'
import { OnboardingShell } from './OnboardingShell'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../api/settingsCache', () => ({
  getSettingsSnapshotCached: vi.fn(),
  importSettingsSnapshotCached: vi.fn(),
  refreshSettingsSnapshot: vi.fn(),
  saveSettingsSnapshotCached: vi.fn(),
  updateSettingsCached: vi.fn(),
}))

const version = { epoch: 'company-test', revision: 7 }
const snapshot = () => ({ settings: makeSettings({ settingsLanguage: 'zh', onboardingStatus: 'pending' }), version })

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(getSettingsSnapshotCached).mockResolvedValue(snapshot())
  vi.mocked(open).mockResolvedValue('/company/config.json')
  vi.mocked(importSettingsSnapshotCached).mockResolvedValue({
    settings: makeSettings({ onboardingStatus: 'completed' }),
    version: { ...version, revision: 8 },
  })
})

function mount() {
  const onComplete = vi.fn()
  const onSettingsChange = vi.fn()
  const result = render(<OnboardingShell onComplete={onComplete} onSkip={vi.fn()} onSettingsChange={onSettingsChange} />)
  return { ...result, onComplete, onSettingsChange }
}

describe('company configuration onboarding', () => {
  it('imports against the loaded version and exits only after the transaction completes, without saving the old draft', async () => {
    const { onComplete, onSettingsChange } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(onComplete).toHaveBeenCalledOnce())
    expect(importSettingsSnapshotCached).toHaveBeenCalledWith('/company/config.json', version, true)
    expect(onSettingsChange).toHaveBeenCalledOnce()
    expect(saveSettingsSnapshotCached).not.toHaveBeenCalled()
  })

  it('cancels without writing and leaves manual setup available', async () => {
    vi.mocked(open).mockResolvedValue(null)
    const { onComplete } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(screen.getByRole('button', { name: '导入公司配置' })).toBeEnabled())
    expect(importSettingsSnapshotCached).not.toHaveBeenCalled()
    expect(onComplete).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '手动配置' }))
    expect(await screen.findByRole('heading', { name: '配置 AI 模型' })).toBeInTheDocument()
  })

  it('keeps failures visible and lets a retry complete', async () => {
    vi.mocked(importSettingsSnapshotCached).mockRejectedValueOnce(new Error('配置文件无效'))
    const { onComplete } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('配置文件无效')
    expect(onComplete).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(onComplete).toHaveBeenCalledOnce())
  })

  it('refreshes the version after a conflict before an explicit retry', async () => {
    const freshVersion = { ...version, revision: 12 }
    vi.mocked(importSettingsSnapshotCached).mockRejectedValueOnce({ code: 'versionConflict', message: 'Settings changed', expectedVersion: version, actualVersion: { ...version, revision: 12 } })
    vi.mocked(refreshSettingsSnapshot).mockResolvedValueOnce({ ...snapshot(), version: freshVersion })
    const { onComplete } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    expect(await screen.findByRole('alert')).toHaveTextContent(/设置已更新/)
    expect(refreshSettingsSnapshot).toHaveBeenCalledOnce()
    expect(onComplete).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(onComplete).toHaveBeenCalledOnce())
    expect(importSettingsSnapshotCached).toHaveBeenLastCalledWith('/company/config.json', freshVersion, true)
  })

  it('blocks navigation and duplicate imports during the picker and import', async () => {
    let finish!: (value: Awaited<ReturnType<typeof importSettingsSnapshotCached>>) => void
    vi.mocked(importSettingsSnapshotCached).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    const { onComplete } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(importSettingsSnapshotCached).toHaveBeenCalledOnce())
    expect(screen.getByRole('button', { name: '正在导入…' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '手动配置' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '跳过引导' })).toBeDisabled()
    await act(async () => finish({ ...snapshot(), settings: makeSettings({ onboardingStatus: 'completed' }) }))
    expect(onComplete).toHaveBeenCalledOnce()
  })

  it('does not navigate or publish stale UI callbacks after unmounting', async () => {
    let finish!: (value: Awaited<ReturnType<typeof importSettingsSnapshotCached>>) => void
    vi.mocked(importSettingsSnapshotCached).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    const { unmount, onComplete, onSettingsChange } = mount()
    fireEvent.click(await screen.findByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(importSettingsSnapshotCached).toHaveBeenCalledOnce())
    unmount()
    await act(async () => finish(snapshot()))
    expect(onComplete).not.toHaveBeenCalled()
    expect(onSettingsChange).not.toHaveBeenCalled()
  })

  it.each(['zh', 'en'] as const)('keeps Dsivio branding and omits external CLI promises in %s', async settingsLanguage => {
    vi.mocked(getSettingsSnapshotCached).mockResolvedValue({ ...snapshot(), settings: makeSettings({ settingsLanguage }) })
    mount()
    expect(await screen.findByRole('heading', { name: settingsLanguage === 'zh' ? '欢迎使用 Dsivio' : 'Welcome to Dsivio' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: settingsLanguage === 'zh' ? '导入公司配置' : 'Import company configuration' })).toBeEnabled()
    expect(screen.queryByText(/Claude Code|external CLI|外部 CLI|Dsivio Chat/)).not.toBeInTheDocument()
    expect(document.querySelectorAll('.onboarding-feature-card')).toHaveLength(6)
  })
})
