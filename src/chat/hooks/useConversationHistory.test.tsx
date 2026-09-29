import { act, renderHook } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { useConversationHistory } from './useConversationHistory'
import { chatApi } from '../api'
import { clearChatReadingPositions, rememberChatReadingPosition } from '../chatReadingPosition'
import { invalidateConversationTransition } from '../conversationTransitionStore'
import type { Conversation } from '../types'

vi.mock('../api', () => ({ chatApi: {
  getConversation: vi.fn(), getConversationWindow: vi.fn(), getConversationRevision: vi.fn(),
} }))
const conversation = (id: string): Conversation => ({ id, revision: 1, title: id,
  provider_id: 'p', model: 'm', created_at: 1, updated_at: 1,
  messages: [{ id: 'u', role: 'user', content: 'question', timestamp: 1 }],
})
beforeEach(() => { vi.resetAllMocks(); clearChatReadingPositions(); invalidateConversationTransition() })

it('uses a display window normally, full history for a saved reading anchor, and validates cache revisions', async () => {
  const full = conversation('a')
  const partial = { ...full, history_start: 1, history_total: 2 }
  vi.mocked(chatApi.getConversation).mockResolvedValue(full)
  vi.mocked(chatApi.getConversationWindow).mockResolvedValue(partial)
  vi.mocked(chatApi.getConversationRevision).mockResolvedValue(1)
  const { result } = renderHook(() => useConversationHistory({ current: full }, { current: 'a' }, vi.fn()))
  expect(await result.current.readConversationWindow('a')).toEqual(partial)
  rememberChatReadingPosition('a', { following: false, rowKey: 'u', rowOffset: 1, scrollTop: 100, layoutKey: '' })
  expect(await result.current.readConversationWindow('a')).toEqual(full)
  result.current.cache.remember(full)
  vi.mocked(chatApi.getConversation).mockClear()
  expect(await result.current.readConversation('a')).toEqual(full)
  expect(chatApi.getConversation).not.toHaveBeenCalled()
  vi.mocked(chatApi.getConversationRevision).mockResolvedValue(2)
  await result.current.readConversation('a')
  expect(chatApi.getConversation).toHaveBeenCalledOnce()
})

it('discards input history and read errors after navigation instead of changing the new composer', async () => {
  const current = { current: conversation('a') }
  const currentId = { current: 'a' }
  const report = vi.fn()
  let resolve!: (value: Conversation) => void
  vi.mocked(chatApi.getConversation).mockImplementationOnce(() => new Promise(done => { resolve = done }))
  const { result } = renderHook(() => useConversationHistory(current, currentId, report))
  const pending = result.current.loadInputHistory()
  invalidateConversationTransition()
  current.current = conversation('b'); currentId.current = 'b'
  resolve(conversation('a'))
  expect(await pending).toBeNull()
  let reject!: (value: Error) => void
  vi.mocked(chatApi.getConversation).mockImplementationOnce(() => new Promise((_done, fail) => { reject = fail }))
  const failed = result.current.loadInputHistory()
  invalidateConversationTransition()
  reject(new Error('late read failure'))
  await act(async () => { expect(await failed).toBeNull() })
  expect(report).not.toHaveBeenCalled()
})

it('returns full user input history and drops cached history on unmount', async () => {
  const full = conversation('a')
  vi.mocked(chatApi.getConversation).mockResolvedValue(full)
  const { result, unmount } = renderHook(() => useConversationHistory({ current: full }, { current: 'a' }, vi.fn()))
  expect(await result.current.loadInputHistory()).toEqual(['question'])
  const cache = result.current.cache
  cache.remember(full)
  unmount()
  expect(cache.stats().entries).toBe(0)
})
