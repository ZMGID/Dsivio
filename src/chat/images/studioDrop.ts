const IMAGE_EXTS = new Set(['png', 'jpg', 'jpeg', 'webp'])

export function looksLikeImagePath(path: string): boolean {
  const name = path.replace(/\\/g, '/').split('/').pop() ?? ''
  const dot = name.lastIndexOf('.')
  if (dot <= 0) return false
  return IMAGE_EXTS.has(name.slice(dot + 1).toLowerCase())
}

/** Folders (or mixed drops) become SKUs; image-only drops append as reference photos. */
export function dropAsProducts(paths: string[], feature: string): boolean {
  return feature === 'client' || paths.some((p) => !looksLikeImagePath(p))
}

export type ImageDropZone = 'examples' | 'products' | 'sources'

export function dropZoneFromElement(node: EventTarget | null): ImageDropZone | null {
  const el = node as { closest?: (selector: string) => unknown } | null
  if (!el || typeof el.closest !== 'function') return null
  if (el.closest('[data-image-drop="examples"]')) return 'examples'
  if (el.closest('[data-image-drop="sources"]')) return 'sources'
  if (el.closest('[data-image-drop="products"]')) return 'products'
  return null
}

export function dropZoneFromPoint(
  x: number,
  y: number,
  atPoint: (x: number, y: number) => Element | null = (left, top) => document.elementFromPoint(left, top),
): ImageDropZone | null {
  return dropZoneFromElement(atPoint(x, y))
}
