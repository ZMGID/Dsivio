/** Shared update notice survives sidebar mode changes and settings dismissal. */
let available = false
const listeners = new Set<() => void>()

export const getUpdateAvailable = () => available
export function subscribeUpdateAvailable(listener: () => void) {
  listeners.add(listener)
  return () => { listeners.delete(listener) }
}
export function publishUpdateAvailability(info: { available: boolean; checkFailed?: boolean }) {
  if (info.checkFailed || available === info.available) return
  available = info.available
  listeners.forEach((listener) => listener())
}
