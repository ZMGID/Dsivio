import { act, cleanup, renderHook, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { MarketSnapshot } from './types'

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), unlisten: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mocks.invoke }))
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }))
vi.mock('../utils', () => ({ isTauriRuntime: () => true }))
const empty: MarketSnapshot = { categories: [], entries: [], installed: [], error: null, refreshedAt: null, sourceUrl: '' }
beforeEach(() => {
  vi.resetModules(); vi.clearAllMocks()
  mocks.invoke.mockResolvedValue(empty)
  mocks.listen.mockResolvedValue(mocks.unlisten)
})
afterEach(cleanup)

it('多个组件共享一次初始化和一组监听，窗口回焦不触发首屏加载', async () => {
  const { useMarket } = await import('./api')
  const hook = renderHook(() => [useMarket(), useMarket()])
  await waitFor(() => expect(hook.result.current[0].initialLoading).toBe(false))
  await waitFor(() => expect(hook.result.current[0].loading).toBe(false))
  expect(mocks.invoke.mock.calls.map(c => c[1].request.action)).toEqual(['snapshot', 'refresh'])
  expect(mocks.listen).toHaveBeenCalledTimes(1)
  await act(async () => { window.dispatchEvent(new Event('focus')) })
  expect(mocks.invoke).toHaveBeenCalledTimes(3)
  expect(hook.result.current[0].initialLoading).toBe(false)
  hook.unmount()
  expect(mocks.unlisten).toHaveBeenCalledTimes(1)
})

it('慢速刷新期间保留已加载的空状态，并合并重复刷新', async () => {
  const { useMarket, marketApi } = await import('./api')
  const hook = renderHook(useMarket)
  await waitFor(() => expect(hook.result.current.initialLoading).toBe(false))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let finish!: (value: MarketSnapshot) => void
  mocks.invoke.mockImplementation(() => new Promise(resolve => { finish = resolve }))
  let first!: Promise<void>, second!: Promise<void>
  act(() => { first = marketApi.refresh(); second = marketApi.refresh() })
  await waitFor(() => expect(hook.result.current.loading).toBe(true))
  expect(hook.result.current.initialLoading).toBe(false)
  expect(hook.result.current.snapshot).toEqual(empty)
  await act(async () => { finish(empty); await Promise.all([first, second]) })
  expect(mocks.invoke).toHaveBeenCalledTimes(3)
})

it('窗口回焦不会抹掉网络错误，成功重试才清除', async () => {
  const { useMarket, marketApi } = await import('./api')
  mocks.invoke.mockImplementation((_cmd, { request }) => request.action === 'refresh' ? Promise.reject('网络失败') : Promise.resolve(empty))
  const hook = renderHook(useMarket)
  await waitFor(() => expect(hook.result.current.snapshot.error).toBe('网络失败'))
  await act(async () => { await marketApi.refresh(false) })
  expect(hook.result.current.snapshot.error).toBe('网络失败')
  mocks.invoke.mockResolvedValue(empty)
  await act(async () => { await marketApi.refresh() })
  expect(hook.result.current.snapshot.error).toBeNull()
})

it('慢速远端响应不会覆盖期间已更新的本地加载状态', async () => {
  const { useMarket, marketApi } = await import('./api')
  const hook = renderHook(useMarket)
  await waitFor(() => expect(hook.result.current.initialLoading).toBe(false))
  await waitFor(() => expect(hook.result.current.loading).toBe(false))
  let finish!: (value: MarketSnapshot) => void
  const updated = { ...empty, installed: [{ id: 'test', enabled: false }] } as MarketSnapshot
  mocks.invoke.mockImplementation((_cmd, { request }) => request.action === 'refresh'
    ? new Promise(resolve => { finish = resolve }) : Promise.resolve(updated))
  let remote!: Promise<void>
  act(() => { remote = marketApi.refresh() })
  await waitFor(() => expect(hook.result.current.loading).toBe(true))
  await act(async () => { await marketApi.refresh(false) })
  await act(async () => { finish(empty); await remote })
  expect(hook.result.current.snapshot.installed).toEqual(updated.installed)
})

it('从安装对话返回市场时重新读取本机安装状态', async () => {
  const { useMarket } = await import('./api')
  const first = renderHook(useMarket)
  await waitFor(() => expect(first.result.current.loading).toBe(false))
  first.unmount()
  const installed = { ...empty, installed: [{ id: 'hypit', status: 'ready' }] } as MarketSnapshot
  mocks.invoke.mockResolvedValue(installed)
  const second = renderHook(useMarket)
  await waitFor(() => expect(second.result.current.snapshot.installed).toEqual(installed.installed))
  expect(mocks.invoke.mock.calls.map(c => c[1].request.action)).toEqual(['snapshot', 'refresh', 'snapshot'])
})

it('内置插件安装和卸载直接调用本地命令并刷新市场', async () => {
  const { marketApi } = await import('./api')
  await marketApi.installBuiltIn('hypit')
  await marketApi.uninstallBuiltIn('hypit')
  expect(mocks.invoke.mock.calls.map(call => call[1].request.action)).toEqual(['install', 'snapshot', 'uninstall', 'snapshot'])
})
