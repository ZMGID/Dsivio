import { useEffect, useRef, useState } from 'react'
import { ChevronDown, Loader2, WandSparkles } from 'lucide-react'
import { IconButton } from '../../components/Button'
import { chatApi } from '../api'
import { canOptimizeComposerText } from '../promptOptimize'
import type { ChatAssistant } from '../types'

export function RequirementOptimize({
  value,
  disabled,
  onChange,
  onError,
}: {
  value: string
  disabled?: boolean
  onChange: (next: string) => void
  onError: (message: string) => void
}) {
  const [assistants, setAssistants] = useState<ChatAssistant[]>([])
  const [assistantId, setAssistantId] = useState('')
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [snapshot, setSnapshot] = useState<string | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    void chatApi
      .getAssistants()
      .then((all) => setAssistants(all.filter((a) => (a.installed ?? true) !== false && !a.archived)))
      .catch(() => setAssistants([]))
  }, [])

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
  const canRun = canOptimizeComposerText(value)

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
        purpose: 'image_brief',
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
        label={busy ? '正在优化' : canUndo ? '撤销优化' : !canRun ? '先写下图片要求' : '优化提示词'}
        disabled={disabled || busy || (!canUndo && !canRun)}
        onClick={() => void optimize()}
      >
        {busy ? <Loader2 size={14} className="is-spinning" /> : <WandSparkles size={14} />}
      </IconButton>
    </div>
  )
}
