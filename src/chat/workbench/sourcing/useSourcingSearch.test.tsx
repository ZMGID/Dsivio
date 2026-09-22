import { act, renderHook, waitFor } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { useSourcingSearch } from './useSourcingSearch'
import type { SourcingSearch } from '../../../generated/sourcing'
vi.mock('../../../api/tauri', () => ({ api: { sourcingHistory: vi.fn(async () => []), sourcingSearch: vi.fn(), sourcingGetSearch: vi.fn() } }))
it('deduplicates a pending search and isolates an abandoned page', async () => {
  let resolve!: (value: SourcingSearch) => void
  vi.mocked(api.sourcingSearch).mockImplementation(() => new Promise(r => { resolve = r }))
  const first = renderHook(useSourcingSearch)
  await act(async () => { void first.result.current.search({ image: 'data', name: 'first', sort: 'relevance', limit: 20, purchaseAmount: 1 }) })
  await act(async () => { void first.result.current.search({ image: 'data', name: 'duplicate', sort: 'relevance', limit: 20, purchaseAmount: 1 }) })
  expect(api.sourcingSearch).toHaveBeenCalledTimes(1)
  first.unmount()
  const next = renderHook(useSourcingSearch)
  await act(async () => { resolve({ id: 'old', name: 'first', fetchedAt: '', products: [], sort: 'relevance', purchaseAmount: 1 }) })
  await waitFor(() => expect(next.result.current.pending).toBe(false))
  expect(next.result.current.result).toBeNull()
  next.unmount()
})

it('keeps a successful search usable when refreshing history fails', async () => {
  vi.mocked(api.sourcingHistory).mockResolvedValueOnce([]).mockRejectedValueOnce('历史读取失败')
  const value: SourcingSearch = { id: 'ok', name: 'image', fetchedAt: '', products: [], sort: 'relevance', purchaseAmount: 1 }
  vi.mocked(api.sourcingSearch).mockResolvedValue(value)
  const hook = renderHook(useSourcingSearch)
  await act(async () => {})
  let success = false
  await act(async () => { success = await hook.result.current.search({ image: 'data', name: 'image', sort: 'relevance', limit: 10, purchaseAmount: 1 }) })
  expect(success).toBe(true)
  expect(hook.result.current.result).toEqual(value)
  expect(hook.result.current.pending).toBe(false)
  expect(hook.result.current.error).toBe('')
  hook.unmount()
})

it('does not let an older history read overwrite the latest refresh', async () => {
  let finishInitial!: (rows: []) => void
  vi.mocked(api.sourcingHistory).mockImplementationOnce(() => new Promise(resolve => { finishInitial = resolve }))
  const latest = [{ id: 'new', name: 'new', fetchedAt: '', count: 0 }]
  vi.mocked(api.sourcingHistory).mockResolvedValueOnce(latest)
  const hook = renderHook(useSourcingSearch)
  await act(async () => { await hook.result.current.refreshHistory() })
  await act(async () => { finishInitial([]) })
  expect(hook.result.current.history).toEqual(latest)
  hook.unmount()
})
