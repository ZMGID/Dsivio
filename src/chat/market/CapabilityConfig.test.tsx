import { fireEvent, render, screen, waitFor, cleanup } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { CapabilityConfig } from './CapabilityConfig'
import { refreshSettings, updateSettingsCached } from '../../api/settingsCache'

vi.mock('../../api/settingsCache', () => ({
  refreshSettings: vi.fn(),
  updateSettingsCached: vi.fn(),
  subscribeSettings: () => () => {},
}))
beforeEach(() => {
  vi.clearAllMocks()
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', '') }
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open') }
  vi.mocked(refreshSettings).mockResolvedValue({ capabilityConfigText: 'original', lang: 'zh' } as never)
  vi.mocked(updateSettingsCached).mockResolvedValue(undefined as never)
})
afterEach(cleanup)

it('saves the latest text before closing while preserving other settings', async () => {
  const onClose = vi.fn()
  render(<CapabilityConfig lang="zh" open onClose={onClose} />)
  const editor = await screen.findByRole('textbox')
  fireEvent.change(editor, { target: { value: 'image\nkey: example' } })
  fireEvent.click(screen.getByRole('button', { name: '关闭能力配置' }))
  await waitFor(() => expect(onClose).toHaveBeenCalledOnce())
  const mutate = vi.mocked(updateSettingsCached).mock.calls[0][0]
  expect(mutate({ lang: 'zh', capabilityConfigText: 'original' } as never)).toEqual({ lang: 'zh', capabilityConfigText: 'image\nkey: example' })
})

it('keeps the dialog and draft when saving fails on close', async () => {
  vi.mocked(updateSettingsCached).mockRejectedValueOnce(new Error('save failed'))
  const onClose = vi.fn()
  render(<CapabilityConfig lang="zh" open onClose={onClose} />)
  fireEvent.change(await screen.findByRole('textbox'), { target: { value: 'keep my text' } })
  fireEvent.click(screen.getByRole('button', { name: '关闭能力配置' }))
  await screen.findByRole('alert')
  expect(onClose).not.toHaveBeenCalled()
  expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('keep my text')
})
