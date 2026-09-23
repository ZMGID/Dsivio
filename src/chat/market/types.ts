export type MarketManifest = {
  schemaVersion: 1
  id: string
  version: string
  name: string
  icon?: string
  summary: string
  categoryIds: string[]
  compatibility: { minAppVersion: string; platforms: string[] }
  notices: string[]
  welcome: string
  inputHint: string
  startPrompt?: string
  setupSkillId?: string
  mainSkillId?: string
  skillIds?: string[]
  checkCommand?: string | null
  verification: { platform: string; dsivioVersion: string; verifiedAt: string; record: string }[]
}
export type MarketExample = {
  schemaVersion: 1
  messages: { role: 'user' | 'assistant'; text: string; attachments?: { type: 'image' | 'video' | 'file'; label: string; path: string; poster?: string }[] }[]
}
export type MarketSource =
  | { repository: string; revision: string; directory: string; kind?: never }
  | { kind: 'local-draft'; revision: string; directory: string; repository?: never }
  | { kind: 'built-in' }
const builtInMarketIds = new Set(['hypit', 'remotion-agent-skills', 'srt-whiteboard-animation', 'feishu-cli'])
export const isBuiltInMarketId = (id: string) => builtInMarketIds.has(id)
export type MarketEntry = { id: string; version: string; source: MarketSource; manifest?: MarketManifest; error?: string }
export type MarketLocal = {
  id: string
  manifest: MarketManifest
  source: MarketSource
  status: 'configuring' | 'ready' | 'failed'
  enabled: boolean
  pluginId: string | null
  skillId: string | null
  conversationId: string | null
  error: string | null
  phase?: 'installing' | 'removing' | 'ready'
}
export type MarketSnapshot = {
  preview?: boolean
  categories: { id: string; name: string }[]
  entries: MarketEntry[]
  installed: MarketLocal[]
  refreshedAt: number | null
  error: string | null
  sourceUrl: string
}
export type MarketItem = { id: string; manifest?: MarketManifest; entry?: MarketEntry; local?: MarketLocal }
export function marketItems(snapshot: MarketSnapshot): MarketItem[] {
  const result = new Map<string, MarketItem>()
  for (const entry of snapshot.entries) result.set(entry.id, { id: entry.id, manifest: entry.manifest, entry })
  for (const local of snapshot.installed) {
    const old = result.get(local.id)
    result.set(local.id, { ...old, id: local.id, manifest: local.manifest, local })
  }
  return [...result.values()]
}
export function primaryAction(item: MarketItem): 'install' | 'repair' | 'continue' | 'enable-use' | 'use' | 'unavailable' {
  if (item.local?.phase === 'removing') return 'unavailable'
  if (item.local?.source?.kind === 'built-in' && item.local.status === 'failed') return 'repair'
  if (item.local?.source?.kind === 'local-draft' && item.entry?.source.kind === 'local-draft' && item.local.source.revision !== item.entry.source.revision) return 'repair'
  if (item.local?.status === 'ready') return item.local.enabled ? 'use' : 'enable-use'
  if (item.local) return 'continue'
  return item.manifest && !item.entry?.error ? 'install' : 'unavailable'
}

export function marketUsePrompt(local: MarketLocal): string {
  return (local.source.kind === 'built-in' ? local.manifest.startPrompt?.trim() : '')
    || '使用这个插件，告诉我可以做什么。'
}
