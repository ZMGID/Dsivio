import { BUILTIN_AGENT_RUNTIME, type AgentRuntimeConfig } from './api'

export const LAST_AGENT_RUNTIME_KEY = 'kivio.chat.lastAgentRuntime'

export function parseLastAgentRuntime(raw: unknown): AgentRuntimeConfig | null {
  if (!raw || typeof raw !== 'object') return null
  const { kind } = raw as { kind?: unknown }
  if (kind === 'builtin' || kind === 'chat' || kind === 'external') return { ...BUILTIN_AGENT_RUNTIME }
  return null
}

export function loadLastAgentRuntime(): AgentRuntimeConfig | null {
  try { return parseLastAgentRuntime(JSON.parse(window.localStorage.getItem(LAST_AGENT_RUNTIME_KEY) ?? 'null')) }
  catch { return null }
}

export function saveLastAgentRuntime(runtime: AgentRuntimeConfig): void {
  try {
    const value = parseLastAgentRuntime(runtime)
    if (value) window.localStorage.setItem(LAST_AGENT_RUNTIME_KEY, JSON.stringify(value))
  } catch { /* Local preferences are best effort. */ }
}
