import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Loader2, WandSparkles, Plus, Pencil, Undo2 } from 'lucide-react'
import { Button, IconButton } from '../../components/Button'
import { chatApi } from '../api'
import { canOptimizeComposerText } from '../promptOptimize'
import { AssistantDialog } from '../AssistantEditor'
import { assistantFitsPurpose } from '../assistantCategories'
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

/** Product photos and workflow sources only — never front/back asset ids. */
export function collectBriefImagePaths(brief: {
  products?: Array<{ assets?: Array<{ path?: string | null | undefined }> }>
  workflowInput?: { sources?: Array<{ path?: string | null | undefined }> } | null
}): string[] {
  return collectStudioMediaPaths([
    ...(brief.products ?? []).flatMap((product) =>
      (product.assets ?? []).map((asset) => asset.path),
    ),
    ...(brief.workflowInput?.sources ?? []).map((source) => source.path),
  ])
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
  onAssistantChange,
}: {
  onAssistantChange?: (id: string) => void
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
  const [snapshot, setSnapshot] = useState<{ original: string; result: string } | null>(null)
  const [editing, setEditing] = useState<ChatAssistant | null | undefined>(undefined)
  const latestValue = useRef(value)
  latestValue.current = value
  const mounted = useRef(true)
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  const menuRef = useRef<HTMLDivElement>(null)
  const includeKey = (includeAssistantIds ?? []).join(',')
  const errorRef = useRef(onError)
  errorRef.current = onError
  const selectionRef = useRef(onAssistantChange)
  selectionRef.current = onAssistantChange
  const resolvedMedia = collectStudioMediaPaths(mediaPaths ?? [])

  useEffect(() => {
    let cancelled = false
    const included = includeKey.split(',')
    void chatApi
      .getAssistants()
      .then((all) => {
        if (cancelled) return
        const listed = all.filter(assistant => !assistant.archived && assistantFitsPurpose(assistant, purpose))
        listed.sort((a, b) => {
          const rank = (id: string) => {
            if (id === preferredAssistantId) return 0
            const idx = included.indexOf(id)
            if (idx >= 0) return idx + 1
            return 100
          }
          const delta = rank(a.id) - rank(b.id)
          if (delta !== 0) return delta
          return a.name.localeCompare(b.name, 'zh')
        })
        setAssistants(listed)
        if (preferredAssistantId && !listed.some(a => a.id === preferredAssistantId)) {
          setAssistantId('')
          selectionRef.current?.('')
        }
      })
      .catch(() => { if (!cancelled) errorRef.current('助手读取失败，请重试') })
    return () => { cancelled = true }
  }, [preferredAssistantId, includeKey, purpose])

  useEffect(() => {
    setAssistantId(preferredAssistantId ?? '')
  }, [preferredAssistantId])

  // Refresh when opening, including assistants created elsewhere in the app.
  useEffect(() => {
    if (!open) return
    void chatApi.getAssistants().then(all => {
      if (mounted.current) setAssistants(all.filter(a => !a.archived && assistantFitsPurpose(a, purpose)))
    }).catch(() => errorRef.current('助手读取失败，请重试'))
  }, [open, purpose])

  const choose = (id: string) => {
    setAssistantId(id)
    onAssistantChange?.(id)
    setOpen(false)
  }

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
  const canUndo = snapshot !== null && snapshot.result === value && snapshot.original !== value
  const canRun =
    resolvedMedia.length > 0
      ? !value.trim().startsWith('/')
      : canOptimizeComposerText(value)
  const emptyHint =
    purpose === 'video_brief' ? '先写下拍摄要求或加载素材' : '先写下图片要求或加载素材'

  const optimize = async () => {
    if (disabled || busy) return
    if (!canRun) return
    const original = value
    setBusy(true)
    try {
      const result = await chatApi.optimizePrompt(original, null, {
        assistantId: selected?.id ?? null,
        purpose,
        mediaPaths: resolvedMedia,
      })
      if (!mounted.current || latestValue.current !== original) return
      setSnapshot({ original, result })
      onChange(result)
    } catch (error) {
      if (mounted.current) onError(error instanceof Error && error.message.trim() ? error.message : '提示词优化失败')
    } finally {
      if (mounted.current) setBusy(false)
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
              choose('')
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
                choose(assistant.id)
              }}
            >
              {assistant.name}
            </button>
          ))}
          {!assistants.length && <p className="px-3 py-2 text-xs text-neutral-500">还没有助手，把专业提示词存进来即可使用。</p>}
          <button type="button" className="kv-menu-item" onClick={() => { setOpen(false); setEditing(null) }}><Plus size={13} />新建助手</button>
          {selected && <button type="button" className="kv-menu-item" onClick={() => { setOpen(false); setEditing(selected) }}><Pencil size={13} />编辑当前助手</button>}
        </div>
      )}
      <Button
        size="sm"
        aria-label={busy ? '正在优化' : !canRun ? emptyHint : '优化提示词'}
        disabled={disabled || busy || !canRun}
        onClick={() => void optimize()}
      >
        {busy ? <Loader2 size={14} className="is-spinning" /> : <WandSparkles size={14} />}
        {busy ? '正在优化' : '优化描述'}
      </Button>
      {canUndo && <IconButton size="sm" label="撤销优化" disabled={disabled || busy} onClick={() => {
        if (snapshot) onChange(snapshot.original)
        setSnapshot(null)
      }}><Undo2 size={14} /></IconButton>}
      {editing !== undefined && <AssistantDialog assistant={editing} onClose={() => setEditing(undefined)} onSaved={saved => {
        setAssistants(current => [saved, ...current.filter(a => a.id !== saved.id)])
        choose(saved.id)
        setEditing(undefined)
      }} />}
    </div>
  )
}
