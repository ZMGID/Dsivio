import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Settings } from '../api/tauri'
import { makeSettings, makeProvider } from '../settings/tabs/testFixtures'
import { i18n } from '../settings/i18n'

const mocks = vi.hoisted(() => ({ open: vi.fn(), get: vi.fn(), import: vi.fn(), save: vi.fn() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: mocks.open }))
vi.mock('../api/settingsCache', () => ({
  getSettingsCached: mocks.get,
  importSettingsCached: mocks.import,
  saveSettingsCached: mocks.save,
}))

import { OnboardingShell } from './OnboardingShell'

const completed = () => makeSettings({
  settingsLanguage: 'zh', onboardingStatus: 'completed', providers: [makeProvider()],
})

beforeEach(() => {
  vi.resetAllMocks()
  mocks.get.mockResolvedValue(makeSettings({ settingsLanguage: 'zh', onboardingStatus: 'pending' }))
  mocks.open.mockResolvedValue('/company/config.json')
  mocks.import.mockResolvedValue(completed())
})
afterEach(cleanup)

async function mount() {
  const onComplete = vi.fn()
  const onSkip = vi.fn()
  const onSettingsChange = vi.fn()
  render(<OnboardingShell onComplete={onComplete} onSkip={onSkip} onSettingsChange={onSettingsChange} />)
  const button = await screen.findByRole('button', { name: '导入公司配置' })
  return { button, onComplete, onSkip, onSettingsChange }
}

describe('first-run company configuration', () => {
  it('imports once, completes setup and never saves the empty welcome-page draft', async () => {
    const { button, onComplete, onSettingsChange, onSkip } = await mount()
    fireEvent.click(button)
    await waitFor(() => expect(onComplete).toHaveBeenCalledOnce())
    expect(mocks.open).toHaveBeenCalledWith({ multiple: false, filters: [{ name: 'JSON', extensions: ['json'] }] })
    expect(mocks.import).toHaveBeenCalledOnce()
    expect(mocks.import).toHaveBeenCalledWith('/company/config.json', true)
    expect(onSettingsChange).toHaveBeenCalledOnce()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(onSkip).not.toHaveBeenCalled()
  })

  it('cancelling the file picker keeps manual setup available without writing settings', async () => {
    mocks.open.mockResolvedValue(null)
    const { button, onComplete } = await mount()
    fireEvent.click(button)
    await waitFor(() => expect(screen.getByRole('button', { name: '导入公司配置' })).toBeEnabled())
    expect(mocks.import).not.toHaveBeenCalled()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(onComplete).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '手动配置' }))
    expect(await screen.findByRole('heading', { name: i18n.zh.onboardingProviderTitle })).toBeInTheDocument()
  })

  it('shows import failure, stays in onboarding and allows a successful retry', async () => {
    mocks.import.mockRejectedValueOnce(new Error('文件不是有效的 JSON'))
    const { button, onComplete } = await mount()
    fireEvent.click(button)
    expect(await screen.findByRole('alert')).toHaveTextContent('文件不是有效的 JSON')
    expect(onComplete).not.toHaveBeenCalled()
    expect(mocks.save).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: '手动配置' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: '导入公司配置' }))
    await waitFor(() => expect(onComplete).toHaveBeenCalledOnce())
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('disables navigation and duplicate imports until the configuration transaction finishes', async () => {
    let finish!: (settings: Settings) => void
    mocks.import.mockImplementationOnce(() => new Promise<Settings>(resolve => { finish = resolve }))
    const { button, onComplete } = await mount()
    fireEvent.click(button)
    await waitFor(() => expect(mocks.import).toHaveBeenCalledOnce())
    expect(screen.getByRole('button', { name: '正在导入…' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '手动配置' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '跳过引导' })).toBeDisabled()
    expect(onComplete).not.toHaveBeenCalled()
    await act(async () => finish(completed()))
    expect(onComplete).toHaveBeenCalledOnce()
  })

  it('offers the same import route in English', async () => {
    mocks.get.mockResolvedValue(makeSettings({ settingsLanguage: 'en', onboardingStatus: 'pending' }))
    render(<OnboardingShell onComplete={vi.fn()} onSkip={vi.fn()} />)
    expect(await screen.findByRole('button', { name: 'Import company configuration' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Set up manually' })).toBeEnabled()
  })
})
