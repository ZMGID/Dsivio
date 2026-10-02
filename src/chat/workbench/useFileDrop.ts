import { useEffect, useRef, useState, type RefObject } from 'react'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useChatRouteActive } from '../chatRouteVisibility'

export const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'bmp'] as const
export const STORE_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp'] as const
export const VIDEO_EXTENSIONS = ['mp4', 'mov', 'm4v', 'webm', 'mkv'] as const
export const AUDIO_EXTENSIONS = ['mp3', 'wav', 'm4a'] as const

export function pathExtension(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot < 0 ? '' : name.slice(dot + 1).toLowerCase()
}

/**
 * 工作台上传区的系统文件拖入。只在所在页面处于前台时监听，且只处理落在 `zone` 内的文件，
 * 同一页有多个上传区时各自按落点判断。返回值表示文件正悬停在该区域上，用于高亮。
 */
export function useFileDrop(
  zone: RefObject<HTMLElement | null>,
  extensions: readonly string[],
  onDrop: (accepted: string[], rejected: string[]) => void,
  disabled = false,
): boolean {
  const routeActive = useChatRouteActive()
  const [over, setOver] = useState(false)
  const latest = useRef({ extensions, onDrop })
  latest.current = { extensions, onDrop }
  const enabled = routeActive && !disabled && typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    let unlisten: (() => void) | undefined
    const inside = (position?: { x: number; y: number }) => {
      const element = zone.current
      if (!element || !position) return false
      const scale = window.devicePixelRatio || 1
      const x = position.x / scale
      const y = position.y / scale
      const box = element.getBoundingClientRect()
      return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom
    }
    try {
      getCurrentWebview()
        .onDragDropEvent((event) => {
          if (cancelled) return
          const payload = event.payload
          if (payload.type === 'leave') {
            setOver(false)
            return
          }
          const hit = inside(payload.position)
          if (payload.type === 'enter' || payload.type === 'over') {
            setOver(hit)
            return
          }
          setOver(false)
          if (!hit) return
          const allowed = latest.current.extensions
          const accepted = payload.paths.filter((path) => allowed.includes(pathExtension(path)))
          latest.current.onDrop(accepted, payload.paths.filter((path) => !accepted.includes(path)))
        })
        .then((fn) => {
          if (cancelled) fn()
          else unlisten = fn
        })
        .catch((error) => console.error('Workbench file drop listen failed:', error))
    } catch (error) {
      console.error('Workbench file drop unavailable:', error)
    }
    return () => {
      cancelled = true
      setOver(false)
      unlisten?.()
    }
  }, [enabled, zone])

  return over
}
