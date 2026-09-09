import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { useSharedDraft } from './useSharedDraft'
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
type Draft = { brief: { request: string } }
let stored: { revision: number; value: Draft | null }
const draft = (request: string): Draft => ({ brief: { request } })
function useHarness(initial = draft(''), paused = false) {
  const [value, setValue] = useState(initial)
  const sync = useSharedDraft('video', 'creation', true, value, setValue, paused)
  return { value, setValue, ...sync }
}
async function tick() { await act(async () => { await vi.advanceTimersByTimeAsync(1000) }) }
beforeEach(() => {
  vi.useFakeTimers()
  stored = { revision: 0, value: null }
  vi.mocked(invoke).mockImplementation(async (_cmd, input) => {
    const args = input as { revision: number; value: Draft | null }
    if (args.value) {
      if (args.revision !== stored.revision) throw new Error('conflict')
      stored = { revision: stored.revision + 1, value: args.value }
    }
    return structuredClone(stored) as never
  })
})
afterEach(() => { vi.useRealTimers(); vi.clearAllMocks() })
describe('shared page/chat draft', () => {
  it('publishes page edits and adopts a later chat edit without echo writes', async () => {
    const { result } = renderHook(() => useHarness())
    await tick()
    act(() => result.current.setValue(draft('page')))
    await tick()
    expect(stored.value).toEqual(draft('page'))
    stored = { revision: stored.revision + 1, value: draft('chat') }
    const revision = stored.revision
    await tick()
    expect(result.current.value).toEqual(draft('chat'))
    await tick()
    expect(stored.revision).toBe(revision)
  })
  it('preserves simultaneous edits until the user chooses a version', async () => {
    const { result } = renderHook(() => useHarness())
    await tick()
    act(() => result.current.setValue(draft('local edit')))
    stored = { revision: 2, value: draft('chat edit') }
    await tick()
    expect(result.current.hasConflict).toBe(true)
    expect(result.current.value).toEqual(draft('local edit'))
    expect(stored.value).toEqual(draft('chat edit'))
    act(() => result.current.keep())
    await tick()
    expect(stored.value).toEqual(draft('local edit'))
  })
  it('protects unsynced local recovery on mount and can load shared version', async () => {
    stored = { revision: 4, value: draft('chat') }
    const { result } = renderHook(() => useHarness(draft('offline')))
    await tick()
    expect(result.current.hasConflict).toBe(true)
    act(() => result.current.reload())
    expect(result.current.value).toEqual(draft('chat'))
    await tick()
    expect(stored.revision).toBe(4)
  })
  it('does not restore an old draft when a generation operation finishes', async () => {
    const { result, rerender } = renderHook(({ paused }) => useHarness(draft(''), paused), { initialProps: { paused: false } })
    await tick()
    rerender({ paused: true })
    act(() => result.current.setValue(draft('generated')))
    await tick()
    expect(stored.value).toEqual(draft(''))
    rerender({ paused: false })
    await tick()
    expect(stored.value).toEqual(draft('generated'))
  })
  it('re-reads after a rejected stale write instead of overwriting chat', async () => {
    const { result } = renderHook(() => useHarness())
    await tick()
    act(() => result.current.setValue(draft('local')))
    vi.mocked(invoke).mockImplementationOnce(async () => structuredClone(stored) as never)
      .mockImplementationOnce(async () => { stored = { revision: 8, value: draft('chat') }; throw new Error('conflict') })
    await tick()
    await tick()
    expect(stored.value).toEqual(draft('chat'))
    expect(result.current.value).toEqual(draft('local'))
    expect(result.current.hasConflict).toBe(true)
  })
})

it('never flushes the next workflow into the previous workflow on navigation', async () => {
  const { rerender } = renderHook(({ entry, value }) => useSharedDraft('video', entry, true, value, () => {}), {
    initialProps: { entry: 'creation', value: draft('creation') },
  })
  await tick()
  vi.mocked(invoke).mockClear()
  rerender({ entry: 'analysis', value: draft('analysis') })
  await tick()
  const writes = vi.mocked(invoke).mock.calls.map(([, args]) => args as { entry: string; value?: Draft })
    .filter(args => args.value)
  expect(writes.some(args => args.entry === 'creation' && args.value?.brief.request === 'analysis')).toBe(false)
})


it('does not rewrite identical JSON whose keys were sorted by the backend', async () => {
  const local = { brief: { request: 'same', name: 'name' } }
  stored = { revision: 3, value: { brief: { name: 'name', request: 'same' } } as Draft }
  const { result } = renderHook(() => useHarness(local))
  await tick()
  await tick()
  expect(result.current.hasConflict).toBe(false)
  expect(stored.revision).toBe(3)
})

it('waits for the shared task to load before saving or leaving the draft', async () => {
  stored = { revision: 6, value: draft('new chat task') }
  let finish: () => void = () => {}
  const applying = new Promise<void>(resolve => { finish = resolve })
  const { unmount } = renderHook(() => useSharedDraft('video', 'creation', true, draft(''), () => applying))
  await tick()
  expect(stored.revision).toBe(6)
  unmount()
  await act(async () => { finish(); await applying })
  expect(stored.value).toEqual(draft('new chat task'))
  expect(stored.revision).toBe(6)
})
