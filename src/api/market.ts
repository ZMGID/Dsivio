import { invoke } from '@tauri-apps/api/core'
import type { PluginDetails, PluginPackage } from './pluginPackages'

export const MARKET_CHANGED_EVENT = 'kivio-market-changed'

export type MarketplacePlugin = {
  name: string
  displayName: string
  description: string
  version: string | null
  category: string
  unavailableReason: string | null
}
export type Marketplace = {
  id: string
  name: string
  description: string
  source: string
  plugins: MarketplacePlugin[]
}

/** User-owned marketplace sources; packageApi remains the owner of installed capabilities. */
export const marketplaceApi = {
  describe: (id: string, plugin: string) => invoke<PluginDetails>('plugin_marketplaces_describe', { id, plugin }),
  list: () => invoke<Marketplace[]>('plugin_marketplaces_list'),
  add: (source: string) => invoke<Marketplace[]>('plugin_marketplaces_add', { source }),
  refresh: (id: string) => invoke<Marketplace[]>('plugin_marketplaces_refresh', { id }),
  remove: (id: string) => invoke<Marketplace[]>('plugin_marketplaces_remove', { id }),
  install: (id: string, plugin: string) => invoke<PluginPackage>('plugin_marketplaces_install', { id, plugin }),
}
