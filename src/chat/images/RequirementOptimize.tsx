import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Loader2, WandSparkles } from 'lucide-react'
import { IconButton } from '../../components/Button'
import { chatApi } from '../api'
import { canOptimizeComposerText } from '../promptOptimize'
import type { ChatAssistant } from '../types'

export type RequirementOptimizePurpose = 'image_brief' | 'video_brief'

const VIDEO_EXT = /\.(mp4|mov|webm|mkv|avi|m4v)$/i

/** Studio 已加载的本地图/视频路径：去重，图片在前，方便模型优先看商品外观。 */
export function collectStudioMediaPaths(
  paths: Array<string | null | undefined>,
): string[] {
  const seen = new Set<string>()
  const images: string[] = []
  const videos: string[] = []
  for (const raw of paths) {
    const path = raw?.trim()
    if (!path || seen.has(path)) continue
    seen.add(path)
    if (VIDEO_EXT.test(path)) videos.push(path)
    else images.push(path)
  }
  return [...images, ...videos]
}

export function RequirementOptimize({
  value,
  disabled,
  onChange,
  onError,
  purpose = 'image_brief',
  preferredAssistantId,
  includeAssistantIds,
  mediaPaths,
}: {
  value: string
  disabled?: boolean
  onChange: (next: string) => void
  onError: (message: string) => void
  purpose?: RequirementOptimizePurpose
  preferredAssistantId?: string
  includeAssistantIds?: string[]
  mediaPaths?: Array<string | null | undefined>
}) {
  const [assistants, setAssistants] = useState<ChatAssistant[]>([])
  const [assistantId, setAssistantId] = useState(preferredAssistantId ?? '')
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [snapshot, setSnapshot] = useState<string | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)
  const includeKey = (includeAssistantIds ?? []).join(',')
  const resolvedMedia = collectStudioMediaPaths(mediaPaths ?? [])

  useEffect(() => {
    const pinned = new Set(
      [preferredAssistantId, ...(includeAssistantIds ?? [])].filter(
        (id): id is string => Boolean(id),
      ),
    )
    void chatApi
      .getAssistants()
      .then((all) => {
        const listed = all.filter((assistant) => {
          if (assistant.archived) return false
          if (pinned.has(assistant.id)) return true
          return (assistant.installed ?? true) !== false
        })
        listed.sort((a, b) => {
          const rank = (id: string) => {
            if (id === preferredAssistantId) return 0
            const idx = includeAssistantIds?.indexOf(id) ?? -1
            if (idx >= 0) return idx + 1
            return 100
          }
          const delta = rank(a.id) - rank(b.id)
          if (delta !== 0) return delta
          return a.name.localeCompare(b.name, 'zh')
        })
        setAssistants(listed)
      })
      .catch(() => setAssistants([]))
  }, [preferredAssistantId, includeKey])

  useEffect(() => {
    if (!preferredAssistantId) return
    if (!assistants.some((assistant) => assistant.id === preferredAssistantId)) return
    setAssistantId((current) => current || preferredAssistantId)
  }, [assistants, preferredAssistantId])

  useEffect(() => {
    if (!open) return
    const onDown = (event: MouseEvent) => {
      if (menuRef.current?.contains(event.target as Node)) return
      setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  const selected = assistants.find((a) => a.id === assistantId) ?? null
  const canUndo = snapshot !== null && snapshot !== value
  const canRun =
    resolvedMedia.length > 0
      ? !value.trim().startsWith('/')
      : canOptimizeComposerText(value)
  const emptyHint =
    purpose === 'video_brief' ? '先写下拍摄要求或加载素材' : '先写下图片要求或加载素材'

  const optimize = async () => {
    if (disabled || busy) return
    if (canUndo) {
      const original = snapshot
      setSnapshot(null)
      if (original !== null) onChange(original)
      return
    }
    if (!canRun) return
    const original = value
    setBusy(true)
    try {
      const result = await chatApi.optimizePrompt(original, null, {
        assistantId: selected?.id ?? null,
        purpose,
        mediaPaths: resolvedMedia,
      })
      setSnapshot(original)
      onChange(result)
    } catch (error) {
      onError(error instanceof Error && error.message.trim() ? error.message : '提示词优化失败')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="is-optimize" ref={menuRef}>
      <button
        type="button"
        className="is-optimize-assistant"
        disabled={disabled || busy}
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        aria-haspopup="listbox"
      >
        {selected?.name || '选择助手'}
        <ChevronDown size={12} />
      </button>
      {open && (
        <div className="kv-menu is-optimize-menu" role="listbox" aria-label="优化用的助手">
          <button
            type="button"
            className="kv-menu-item"
            role="option"
            aria-selected={!selected}
            onClick={() => {
              setAssistantId('')
              setOpen(false)
            }}
          >
            默认优化
          </button>
          {assistants.map((assistant) => (
            <button
              type="button"
              className="kv-menu-item"
              role="option"
              aria-selected={assistant.id === selected?.id}
              key={assistant.id}
              onClick={() => {
                setAssistantId(assistant.id)
                setOpen(false)
              }}
            >
              {assistant.name}
            </button>
          ))}
        </div>
      )}
      <IconButton
        size="sm"
        label={busy ? '正在优化' : canUndo ? '撤销优化' : !canRun ? emptyHint : '优化提示词'}
        disabled={disabled || busy || (!canUndo && !canRun)}
        onClick={() => void optimize()}
      >
        {busy ? <Loader2 size={14} className="is-spinning" /> : <WandSparkles size={14} />}
      </IconButton>
    </div>
  )
}
