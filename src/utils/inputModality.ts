/**
 * WebKit draws its focus ring whenever script moves focus (dialogs, menus), even right after a
 * mouse click. Track the last real input so CSS can keep rings for keyboard navigation only.
 */
const NAVIGATION_KEYS = new Set(['Tab', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'])

export function installInputModality(root: HTMLElement = document.documentElement): () => void {
  const set = (modality: 'pointer' | 'keyboard') => { root.dataset.inputModality = modality }
  const onKey = (event: KeyboardEvent) => { if (NAVIGATION_KEYS.has(event.key)) set('keyboard') }
  const onPointer = () => set('pointer')
  set('pointer')
  document.addEventListener('keydown', onKey, true)
  document.addEventListener('pointerdown', onPointer, true)
  return () => {
    document.removeEventListener('keydown', onKey, true)
    document.removeEventListener('pointerdown', onPointer, true)
  }
}
