import { afterEach, describe, expect, it, vi } from 'vitest'
import { listen } from '@tauri-apps/api/event'
import { installTauriEventUnlistenGuard } from './tauriEventCompatibility'

function runtime() {
  vi.stubGlobal('window', globalThis)
  const listeners: Record<string, Record<number, { handlerId: number }>> = { test: {} }
  const unregisterCallback = vi.fn()
  const invoke = vi.fn(async (command: string) => command === 'plugin:event|listen' ? 7 : undefined)
  vi.stubGlobal('__TAURI_INTERNALS__', { invoke, transformCallback: () => 42, unregisterCallback })
  vi.stubGlobal('__internal_unstable_listeners_object_id__', listeners)
  // Tauri 2.10.1's actual generated cleanup behavior, including its missing guard.
  vi.stubGlobal('__TAURI_EVENT_PLUGIN_INTERNALS__', {
    unregisterListener: (event: string, id: number) => {
      if (listeners[event]) unregisterCallback(listeners[event][id].handlerId)
    },
  })
  return { listeners, invoke, unregisterCallback }
}

afterEach(() => vi.unstubAllGlobals())

describe('Tauri event cleanup compatibility', () => {
  it('still releases the backend listener before the registration eval arrives', async () => {
    const { invoke } = runtime()
    installTauriEventUnlistenGuard()
    const unlisten = await listen('test', () => {})
    await expect(unlisten()).resolves.toBeUndefined()
    expect(invoke).toHaveBeenCalledWith('plugin:event|unlisten', { event: 'test', eventId: 7 }, undefined)
  })

  it('preserves callback cleanup for registered entries', async () => {
    const { listeners, unregisterCallback } = runtime()
    installTauriEventUnlistenGuard()
    const unlisten = await listen('test', () => {})
    listeners.test[7] = { handlerId: 42 }
    await unlisten()
    expect(unregisterCallback).toHaveBeenCalledWith(42)
  })

  it('is a no-op in the browser', () => {
    expect(() => installTauriEventUnlistenGuard()).not.toThrow()
  })
})
