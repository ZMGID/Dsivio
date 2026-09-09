import { looksLikeImagePath } from '../images/studioDrop'

const VIDEO_EXTS = new Set(['mp4', 'mov', 'webm', 'mkv', 'avi', 'm4v'])
const AUDIO_EXTS = new Set(['mp3', 'wav', 'm4a', 'aac', 'ogg', 'flac'])
const REFERENCE_CAP = 3

export type VideoDropZone = 'images' | 'source' | 'referenceVideos' | 'referenceAudios'

export type VideoDropState = {
  images: string[]
  source: string
  referenceVideos?: string[]
  referenceAudios?: string[]
}

export function looksLikeVideoPath(path: string): boolean {
  return hasExtension(path, VIDEO_EXTS)
}

export function looksLikeAudioPath(path: string): boolean {
  return hasExtension(path, AUDIO_EXTS)
}

function hasExtension(path: string, allowed: Set<string>): boolean {
  const name = path.replace(/\\/g, '/').split('/').pop() ?? ''
  const dot = name.lastIndexOf('.')
  if (dot <= 0) return false
  return allowed.has(name.slice(dot + 1).toLowerCase())
}

export function videoDropZoneFromElement(node: EventTarget | null): VideoDropZone | null {
  const el = node as { closest?: (selector: string) => unknown } | null
  if (!el || typeof el.closest !== 'function') return null
  if (el.closest('[data-video-drop="images"]')) return 'images'
  if (el.closest('[data-video-drop="source"]')) return 'source'
  if (el.closest('[data-video-drop="referenceVideos"]')) return 'referenceVideos'
  if (el.closest('[data-video-drop="referenceAudios"]')) return 'referenceAudios'
  return null
}

export function videoDropZoneFromPoint(
  x: number,
  y: number,
  atPoint: (x: number, y: number) => Element | null = (left, top) =>
    document.elementFromPoint(left, top),
): VideoDropZone | null {
  return videoDropZoneFromElement(atPoint(x, y))
}

function appendUnique(current: string[] | undefined, added: string[], cap: number): string[] {
  return [...new Set([...(current || []), ...added])].slice(0, cap)
}

export function applyVideoStudioDrop(
  view: 'creation' | 'analysis' | 'remake' | 'templates' | 'settings' | 'tasks',
  current: VideoDropState,
  paths: string[],
  zone: VideoDropZone | null = null,
): Partial<VideoDropState> | { error: string } {
  if (zone === 'referenceVideos') {
    const videos = paths.filter(looksLikeVideoPath)
    if (!videos.length) return { error: '请拖入参考视频（MP4 / MOV）' }
    const referenceVideos = appendUnique(current.referenceVideos, videos, REFERENCE_CAP)
    if (referenceVideos.length === (current.referenceVideos || []).length) {
      return { error: '参考视频最多 3 段' }
    }
    return { referenceVideos }
  }
  if (zone === 'referenceAudios') {
    const audios = paths.filter(looksLikeAudioPath)
    if (!audios.length) return { error: '请拖入参考音频（MP3 / WAV）' }
    const referenceAudios = appendUnique(current.referenceAudios, audios, REFERENCE_CAP)
    if (referenceAudios.length === (current.referenceAudios || []).length) {
      return { error: '参考音频最多 3 段' }
    }
    return { referenceAudios }
  }
  if (zone === 'source' || (view === 'analysis' && zone !== 'images')) {
    const video = paths.find(looksLikeVideoPath)
    if (!video) return { error: '请拖入视频文件（MP4 / MOV / WebM）' }
    return { source: video }
  }
  if (view === 'remake') {
    if (zone === 'images') {
      const images = [...new Set([...current.images, ...paths.filter(looksLikeImagePath)])]
      if (images.length === current.images.length) return { error: '请拖入商品图片（PNG / JPG / WebP）' }
      return { images }
    }
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
