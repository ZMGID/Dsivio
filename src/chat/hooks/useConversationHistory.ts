import { useCallback, useEffect, useState, type RefObject } from 'react'
import { chatApi } from '../api'
import { forgetChatReadingPosition, recallChatReadingPosition } from '../chatReadingPosition'
import { isPartialConversation } from '../conversationHistoryWindow'
import { createConversationWarmCache } from '../conversationWarmCache'
import { captureConversationNavigation, isCurrentConversationNavigation } from '../conversationTransitionStore'
import type { Conversation } from '../types'

/** Owns display-history caching, read policy and errors across conversation navigation. */
export function useConversationHistory(
  current: RefObject<Conversation | null>,
  currentId: RefObject<string | null>,
  reportInputError: (id: string, message: string) => void,
) {
  const [cache] = useState(createConversationWarmCache)
  const [historyLoadError, setHistoryLoadError] = useState<{ conversationId: string; message: string } | null>(null)
  useEffect(() => () => cache.clear(), [cache])

  const readConversation = useCallback(async (id: string) => {
    const cached = await cache.get(id, chatApi.getConversationRevision).catch(() => null)
    return cached && !isPartialConversation(cached) ? cached : chatApi.getConversation(id)
  }, [cache])
  const readConversationWindow = useCallback(async (id: string) => {
    const cached = await cache.get(id, chatApi.getConversationRevision).catch(() => null)
    if (cached) return cached
    const position = recallChatReadingPosition(id)
    return position && !position.following ? chatApi.getConversation(id) : chatApi.getConversationWindow(id)
  }, [cache])
  const forgetHistory = useCallback((id: string) => {
    cache.forget(id)
    forgetChatReadingPosition(id)
  }, [cache])
  const reportHistoryError = useCallback((conversationId: string, message: string | null) => {
    setHistoryLoadError(message ? { conversationId, message } : null)
  }, [])
  const loadInputHistory = useCallback(async () => {
    const conversation = current.current
    if (!conversation) return null
    const lease = captureConversationNavigation()
    const canCommit = () => isCurrentConversationNavigation(lease) && currentId.current === conversation.id
    try {
      const complete = await chatApi.getConversation(conversation.id)
      return canCommit() ? complete.messages.filter(message => message.role === 'user').map(message => message.content) : null
    } catch (error) {
      if (canCommit()) reportInputError(conversation.id, '读取输入历史失败，请重试。')
      console.error('Failed to load input history:', error)
      return null
    }
  }, [current, currentId, reportInputError])
  return { cache, historyLoadError, readConversation, readConversationWindow, forgetHistory, reportHistoryError, loadInputHistory }
}
