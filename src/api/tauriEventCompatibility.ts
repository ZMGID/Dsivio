/** Backport the missing-entry guard from tauri-apps/tauri#15800 for our
 * pinned Tauri 2.10.1 runtime. Remove when Cargo.lock includes that fix.
 * The SDK must still send plugin:event|unlisten when the registration eval
 * has not arrived; catching the resulting rejection would leak the listener.
 * Keeping this at bootstrap also covers Window.onFocusChanged's SDK listeners.
 */
export function installTauriEventUnlistenGuard(): void {
  if (typeof window === 'undefined') return
  const runtime = window as unknown as {
    __TAURI_EVENT_PLUGIN_INTERNALS__?: { unregisterListener: (event: string, id: number) => void }
    __internal_unstable_listeners_object_id__?: Record<string, Record<number, unknown>>
  }
  const plugin = runtime.__TAURI_EVENT_PLUGIN_INTERNALS__
  if (!plugin) return
  const unregister = plugin.unregisterListener
  plugin.unregisterListener = (event, id) => {
    if (runtime.__internal_unstable_listeners_object_id__?.[event]?.[id]) {
      unregister.call(plugin, event, id)
    }
  }
}
