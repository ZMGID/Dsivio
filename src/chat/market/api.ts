import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useEffect, useSyncExternalStore } from 'react'
import { isTauriRuntime } from '../utils'
import type { MarketExample, MarketLocal, MarketSnapshot } from './types'

const EMPTY: MarketSnapshot = { categories: [], entries: [], installed: [], refreshedAt: null, error: null, sourceUrl: '' }
let snapshot = EMPTY
let loading = false
let hasLoaded = false
let bootstrap: Promise<void> | null = null
let bootstrapped = false
let remoteRequest: Promise<void> | null = null
let localRequest: Promise<void> | null = null
let localRevision = 0
let version = 0
const subscribers = new Set<() => void>()
const notify = () => { version++; subscribers.forEach(fn => fn()) }
async function call<T>(action: string, extra: Record<string, unknown> = {}): Promise<T> {
  if (!isTauriRuntime()) {
    if (action === 'snapshot' || action === 'refresh') return { ...EMPTY, preview: true } as T
    throw new Error('请在 dsivio 桌面应用中使用市场；浏览器预览不执行安装。')
  }
  return invoke<T>('market_command', { request: { action, ...extra } })
}
export const marketApi = {
  icon: (id: string) => call<string>('icon', { id }),
  async refresh(remote = true): Promise<void> {
    if (remote) {
      if (remoteRequest) return remoteRequest
      loading = true; notify()
      const revisionAtStart = localRevision
      remoteRequest = Promise.resolve().then(async () => {
        try {
          const next = await call<MarketSnapshot>('refresh')
          snapshot = localRevision === revisionAtStart ? next : { ...next, installed: snapshot.installed }
          hasLoaded = true
        } catch (e) { snapshot = { ...snapshot, error: String(e) }; hasLoaded = true }
        finally { loading = false; remoteRequest = null; notify() }
      })
      return remoteRequest
    }
    // A mutation notification arriving during a read needs a fresh read afterward.
    if (localRequest) {
      await localRequest
      return this.refresh(false)
    }
    localRequest = Promise.resolve().then(async () => {
      try {
        const next = await call<MarketSnapshot>('snapshot')
        snapshot = hasLoaded ? { ...snapshot, installed: next.installed } : next
        localRevision++; hasLoaded = true
      } catch (e) { snapshot = { ...snapshot, error: String(e) }; hasLoaded = true }
      finally { localRequest = null; notify() }
    })
    return localRequest
  },
  async setEnabled(id: string, enabled: boolean) {
    await call<MarketLocal>('set_enabled', { id, enabled }); await this.refresh(false)
  },
  async installBuiltIn(id: string) {
    await call<MarketLocal>('install', { id }); await this.refresh(false)
  },
  async uninstallBuiltIn(id: string) {
    await call<{ removed: boolean }>('uninstall', { id }); await this.refresh(false)
  },
  async prepare(id: string): Promise<{ brief: string; local: MarketLocal }> {
    const result = await call<{ brief: string; local: MarketLocal }>('prepare', { id })
    await this.refresh(false); return result
  },
  async attachConversation(id: string, conversationId: string) {
    await call('attach_conversation', { id, conversationId }); await this.refresh(false)
  },
  detail: (id: string, local = false) => call<{ example: MarketExample; assetBase: string }>('detail', { id, local }),
  prepareRemove: (id: string, conversationId: string) => call<{ brief: string }>('prepare_remove', { id, conversationId }),
  async discard(id: string) {
    await call('discard', { id }); await this.refresh(false)
  },
}
let observerCount = 0
let removeObservers: (() => void) | undefined
function observeMarket() {
  observerCount++
  if (observerCount === 1 && isTauriRuntime()) {
    let disposed = false
    let unlisten: (() => void) | undefined
    const refreshLocal = () => { void marketApi.refresh(false) }
    void listen('kivio-configuration-changed', refreshLocal)
      .then(fn => { if (disposed) fn(); else unlisten = fn })
      .catch(() => { /* Window focus still refreshes local installation state. */ })
    window.addEventListener('focus', refreshLocal)
    removeObservers = () => { disposed = true; unlisten?.(); window.removeEventListener('focus', refreshLocal) }
  }
  return () => { if (--observerCount === 0) { removeObservers?.(); removeObservers = undefined } }
}
export function useMarket() {
  useSyncExternalStore(fn => { subscribers.add(fn); return () => { subscribers.delete(fn) } }, () => version)
  useEffect(() => {
    if (!bootstrap) bootstrap = marketApi.refresh(false).then(() => marketApi.refresh(true)).finally(() => { bootstrapped = true })
    else if (bootstrapped) void marketApi.refresh(false)
    return observeMarket()
  }, [])
  return { snapshot, loading, initialLoading: !hasLoaded }
}
