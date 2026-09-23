import { memo } from 'react'
import { StreamDotLogo } from './StreamDotLogo'

// Agent conversations can contain heavy tool output in just one message.
// Show feedback from the first render instead of predicting cost by row count.
export const ConversationLoadingState = memo(function ConversationLoadingState() {
  return (
    <div
      className="chat-conversation-loading absolute inset-0 z-30 flex items-center justify-center"
      role="status"
      aria-label="正在加载对话"
    >
      <StreamDotLogo size={104} />
      <span className="sr-only">正在加载对话…</span>
    </div>
  )
})
