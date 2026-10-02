import { draftKey, setComposerDraft } from './composerDraft'

type Listener = () => void

let listener: Listener | null = null

/**
 * 工作台「用对话做」→ 侧栏切到对话并新开一条会话。
 * 和 `composerInsert` 一样，主窗口同时只有一个侧栏，单监听足够。
 */
export function onRequestNewChat(listenerToAdd: Listener): () => void {
  listener = listenerToAdd
  return () => {
    if (listener === listenerToAdd) listener = null
  }
}

/**
 * 把提示词放进新会话草稿，再请侧栏打开对话。
 * 输入框挂上后读 `__new__` 草稿，这里不发送。
 */
export function requestNewChat(input: string): void {
  setComposerDraft(draftKey(null), { input, quotes: [], attachments: [] })
  listener?.()
}
