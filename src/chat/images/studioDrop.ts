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
