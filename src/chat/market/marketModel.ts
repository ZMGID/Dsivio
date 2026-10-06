import { decodeChatRouteId, encodeChatRouteId } from '../routeCodec'
import { hashPath } from '../browserRoute'
import claudeIcons from './claude-market-icons.json'

/** Supplemental HTTPS logos verified against ZCode's Claude catalog, 2026-09-29.
 * Match the actual repository source so another market with the same names keeps its own identity.
 */
export function claudeMarketplaceIcon(source: string | undefined, name: string): string | undefined {
  if (!source) return undefined
  try {
    const url = new URL(source)
    const repository = url.pathname.replace(/\/$/, '').replace(/\.git$/, '')
    if (url.protocol !== 'https:' || url.hostname !== 'github.com' || url.username || url.password
      || repository !== '/anthropics/claude-plugins-official') return undefined
    return Object.prototype.hasOwnProperty.call(claudeIcons, name) ? (claudeIcons as Record<string, string>)[name] : undefined
  } catch { return undefined }
}

export const MARKET_ROUTE = 'chat/market/external'

export function marketHash(): string {
  return `#${MARKET_ROUTE}`
}

export function marketDetailHash(id: string): string {
  return `#${encodeChatRouteId(`${MARKET_ROUTE}/`, id)}`
}

/** `#chat/plugins/{id}` 的插件 id；列表页返回 null。 */
export function marketPluginIdFromHash(): string | null {
  return decodeChatRouteId(`${MARKET_ROUTE}/`, hashPath())
}
