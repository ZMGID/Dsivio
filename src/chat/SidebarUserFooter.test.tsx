import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import { api, type UpdateInfo } from '../api/tauri'
import { publishUpdateAvailability } from '../api/updateAvailability'
import { SidebarUserFooter } from './SidebarUserFooter'

vi.mock('../api/settingsCache', () => ({ getSettingsCached: vi.fn(async () => ({ chat: {} })) }))
vi.mock('../api/tauri', () => ({ api: { onUpdateAvailable: vi.fn() } }))
let receive: (info: UpdateInfo) => void
const dispose = vi.fn()
beforeEach(() => {
  publishUpdateAvailability({ available: false })
  dispose.mockClear()
  vi.mocked(api.onUpdateAvailable).mockImplementation(async (listener) => { receive = listener; return dispose })
})
function footer(onOpenSettings = vi.fn()) {
  return <SidebarUserFooter lang="zh" settingsActive={false} onOpenSettings={onOpenSettings} onSelectLang={vi.fn()} onOpenUsage={vi.fn()} />
}
it('shows a notice for updates, retains it on click/remount, and clears after a successful no-update check', async () => {
  const open = vi.fn()
  const first = render(footer(open))
  expect(screen.queryByRole('button', { name: '发现新版本' })).toBeNull()
  await act(async () => { receive({ available: true, version: '1.0.2' }) })
  expect(screen.getByRole('button', { name: '发现新版本' })).toBeInTheDocument()
  await userEvent.click(screen.getByRole('button', { name: '发现新版本' }))
  expect(open).toHaveBeenCalledOnce()
  expect(screen.getByRole('button', { name: /设置/ })).not.toBe(screen.getByRole('button', { name: '发现新版本' }))
  first.unmount()
  expect(dispose).toHaveBeenCalledOnce()
  render(footer())
  expect(screen.getByRole('button', { name: '发现新版本' })).toBeInTheDocument()
  act(() => publishUpdateAvailability({ available: false, checkFailed: true }))
  expect(screen.getByRole('button', { name: '发现新版本' })).toBeInTheDocument()
  act(() => publishUpdateAvailability({ available: false }))
  expect(screen.queryByRole('button', { name: '发现新版本' })).toBeNull()
})
it('does not show an upgrade button on failed checks and cleans up a late subscription', async () => {
  let resolve!: (dispose: () => void) => void
  vi.mocked(api.onUpdateAvailable).mockReturnValueOnce(new Promise((done) => { resolve = done }))
  const view = render(footer())
  act(() => publishUpdateAvailability({ available: false, checkFailed: true }))
  expect(screen.queryByRole('button', { name: '发现新版本' })).toBeNull()
  view.unmount()
  await act(async () => { resolve(dispose) })
  expect(dispose).toHaveBeenCalledOnce()
})
