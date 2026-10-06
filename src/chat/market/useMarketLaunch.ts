import { useCallback, type MutableRefObject } from 'react'
import { marketApi } from './api'
import { marketUseReusesConversation, marketUseTarget, type MarketLocal } from './types'
import { chatApi } from '../api'
import type { AgentRuntimeConfig, ChatProject, Conversation, PendingAttachment } from '../types'

/** Owns plugin launch: enable, setup selection, project binding and first message. */
export function useMarketLaunch({ activeProviderId, activeModel, usesExternalRuntime, usesChatRuntime,
  draftAgentRuntime, currentConversation, isCurrentConversationBusy, selectedProject, loadSkills,
  applyConversation, syncConversationRoute, refreshSidebar, handleSendMessage, currentConversationIdRef, setChatView,
}: {
  activeProviderId: string; activeModel: string; usesExternalRuntime: boolean; usesChatRuntime: boolean
  draftAgentRuntime: AgentRuntimeConfig; currentConversation: Conversation | null
  isCurrentConversationBusy: () => boolean; selectedProject: ChatProject | null
  loadSkills: () => Promise<void>; applyConversation: (conversation: Conversation) => void
  syncConversationRoute: (id: string) => void; refreshSidebar: () => void
  handleSendMessage: (prompt: string, attachments: PendingAttachment[], options: { conversationOverride: Conversation; activeSkillId?: string }) => Promise<boolean>
  currentConversationIdRef: MutableRefObject<string | null>; setChatView: (view: 'conversation') => void
}) {
  return useCallback(async (item: MarketLocal, newChat: boolean) => {
    if (usesExternalRuntime || usesChatRuntime || draftAgentRuntime.kind !== 'builtin') throw new Error('请先切换到内置 Agent 模式，再使用应用。')
    if (item.status !== 'ready') throw new Error('应用尚未完成安装验收。')
    if (!activeProviderId || !activeModel) throw new Error('请先配置对话模型，再使用应用。')
    const startingHash = window.location.hash
    if (!item.enabled || item.id === 'ziniao-cli') await marketApi.setEnabled(item.id, true)
    const target = marketUseTarget(item, item.manifest.setupSkillId ? await marketApi.setupDone(item.id) : true)
    await loadSkills()
    if (window.location.hash !== startingHash) return
    const pluginProject = item.projectContext ?? (item.manifest.project ? await marketApi.ensureProject(item.id) : null)
    const useProject = pluginProject ?? (selectedProject ? { id: selectedProject.id, name: selectedProject.name } : null)
    const reuse = marketUseReusesConversation(newChat, currentConversation, pluginProject?.id ?? null)
    if (reuse && isCurrentConversationBusy()) throw new Error('请等本次回复结束后再切换应用。')
    let conv = reuse && currentConversation ? currentConversation : await chatApi.createConversation(activeProviderId || undefined, activeModel || undefined, useProject?.name, useProject?.id ?? null)
    conv = await chatApi.updateConversation(conv.id, { activeSkillId: target.skillId, assistantId: null, ...(newChat ? { title: item.manifest.name } : {}) })
    if (window.location.hash !== startingHash) return
    currentConversationIdRef.current = conv.id
    applyConversation(conv)
    setChatView('conversation')
    syncConversationRoute(conv.id)
    refreshSidebar()
    const accepted = await handleSendMessage(target.prompt, [], { conversationOverride: conv, activeSkillId: target.skillId })
    if (!accepted) throw new Error('启动消息未发送，请在对话中重试。')
  }, [activeProviderId, activeModel, usesExternalRuntime, usesChatRuntime, draftAgentRuntime.kind, currentConversation, isCurrentConversationBusy, selectedProject, loadSkills, applyConversation, syncConversationRoute, refreshSidebar, handleSendMessage, currentConversationIdRef, setChatView])

}
