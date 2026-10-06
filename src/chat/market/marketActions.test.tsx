import { act, renderHook, waitFor } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { useMarketAction } from './marketActions'

it('shares an in-flight install and its failure across leaving and reopening the market', async () => {
  let reject!: (error: Error) => void
  const install = vi.fn(() => new Promise<void>((_, fail) => { reject = fail }))
  const actions = { onInstall: install, onUse: vi.fn() }
  const first = renderHook(() => useMarketAction(actions))
  let pending!: Promise<void>
  act(() => { pending = first.result.current.run('remount-test', install) })
  await waitFor(() => expect(first.result.current.busyIds.has('remount-test')).toBe(true))
  first.unmount()
  const second = renderHook(() => useMarketAction(actions))
  expect(second.result.current.busyIds.has('remount-test')).toBe(true)
  act(() => { void second.result.current.run('remount-test', install) })
  expect(install).toHaveBeenCalledTimes(1)
  await act(async () => { reject(new Error('安装失败')); await pending })
  expect(second.result.current.busyIds.has('remount-test')).toBe(false)
  expect(second.result.current.error).toContain('安装失败')
})
