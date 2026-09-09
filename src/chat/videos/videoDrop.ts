import { looksLikeImagePath } from '../images/studioDrop'

const VIDEO_EXTS = new Set(['mp4', 'mov', 'webm', 'mkv', 'avi', 'm4v'])

export function looksLikeVideoPath(path: string): boolean {
  const name = path.replace(/\\/g, '/').split('/').pop() ?? ''
  const dot = name.lastIndexOf('.')
  if (dot <= 0) return false
  return VIDEO_EXTS.has(name.slice(dot + 1).toLowerCase())
}

export function applyVideoStudioDrop(
  view: 'creation' | 'analysis' | 'remake' | 'templates' | 'settings',
  current: { images: string[]; source: string },
  paths: string[],
): { images?: string[]; source?: string } | { error: string } {
  if (view === 'remake') {
    const video = paths.find(looksLikeVideoPath)
    const images = [...new Set([...current.images, ...paths.filter(looksLikeImagePath)])]
    if (!video && images.length === current.images.length) return { error: '请拖入参考视频或商品图片' }
    return { images, ...(video ? { source: video } : {}) }
  }
  if (view === 'analysis') {
    const video = paths.find(looksLikeVideoPath)
    if (!video) return { error: '请拖入视频文件（MP4 / MOV / WebM）' }
    return { source: video }
  }
  if (view !== 'creation') return { error: '当前页不能导入素材' }
  const images = [...new Set([...current.images, ...paths.filter(looksLikeImagePath)])]
  if (images.length === current.images.length) {
    return { error: '请拖入商品图片（PNG / JPG / WebP）' }
  }
  return { images }
}
