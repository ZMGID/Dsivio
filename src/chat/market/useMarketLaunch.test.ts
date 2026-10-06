// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { useMarketLaunch } from './useMarketLaunch'
import { chatApi } from '../api'
import { marketApi } from './api'
import type { MarketLocal } from './types'
import type { Conversation } from '../types'

vi.mock('../api', () => ({ chatApi: { createConversation: vi.fn(), updateConversation: vi.fn() } }))
vi.mock('./api', () => ({ marketApi: { setEnabled: vi.fn(), setupDone: vi.fn(), ensureProject: vi.fn() } }))
it('launches an enabled package without a main Skill through the native conversation flow', async () => {
  const conversation = { id: 'new-package-chat' } as Conversation
  vi.mocked(chatApi.createConversation).mockResolvedValue(conversation)
  vi.mocked(chatApi.updateConversation).mockResolvedValue(conversation)
  const send = vi.fn().mockResolvedValue(true)
  const apply = vi.fn()
  const { result } = renderHook(() => useMarketLaunch({
    activeProviderId: 'provider', activeModel: 'model', usesExternalRuntime: false, usesChatRuntime: false,
    draftAgentRuntime: { kind: 'builtin' }, currentConversation: null, isCurrentConversationBusy: () => false,
    selectedProject: null, loadSkills: vi.fn().mockResolvedValue(undefined), applyConversation: apply,
    syncConversationRoute: vi.fn(), refreshSidebar: vi.fn(), handleSendMessage: send,
    currentConversationIdRef: { current: null }, setChatView: vi.fn(),
  }))
  const plugin = { id: 'native-package', status: 'ready', enabled: true, skillId: null,
    source: { kind: 'local-draft', directory: '/plugins/native', revision: '1' },
    manifest: { name: 'Native package' } } as MarketLocal
  await act(async () => { await result.current(plugin, true) })
  expect(apply).toHaveBeenCalledWith(conversation)
  expect(send).toHaveBeenCalledWith('使用这个插件，告诉我可以做什么。', [], expect.objectContaining({ conversationOverride: conversation }))
  expect(marketApi.setupDone).not.toHaveBeenCalled()
  expect(marketApi.ensureProject).not.toHaveBeenCalled()
})
