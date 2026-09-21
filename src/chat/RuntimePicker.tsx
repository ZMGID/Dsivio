import { memo } from 'react'
import type { AgentRuntimeConfig } from './types'
import './runtimePicker.css'

interface RuntimePickerProps {
  agentRuntime: AgentRuntimeConfig
  onRuntimeChange: (runtime: AgentRuntimeConfig) => void
  conversationId?: string | null
  locked?: boolean
}

/** A single runtime needs a label, not a mode switch. */
export const RuntimePicker = memo(function RuntimePicker({ agentRuntime }: RuntimePickerProps) {
  const label = agentRuntime.kind === 'external'
    ? `${agentRuntime.externalAgentId ?? 'CLI'} · 历史记录`
    : 'Dsivio Agent'
  return (
    <div className="kv-runtime-picker" data-tauri-drag-region="false">
      <span className="kv-runtime-picker__identity" title={label} aria-label={label}>
        <img src="/icon.png" alt="" aria-hidden="true" width={18} height={18}
          className="kv-runtime-picker__builtin-logo" draggable={false} />
      </span>
    </div>
  )
})
