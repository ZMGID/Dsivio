import { useEffect, useId, useRef } from 'react'
import { createPortal } from 'react-dom'
import { Button } from '../../components/Button'

export function TaskDeleteDialog({ count, name, busy, error, onCancel, onConfirm }: {
  count: number; name?: string; busy: boolean; error: string
  onCancel: () => void; onConfirm: () => void
}) {
  const titleId = useId()
  const descriptionId = useId()
  const panel = useRef<HTMLDivElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    cancel.current?.focus()
    return () => { if (previous?.isConnected) previous.focus() }
  }, [])
  return createPortal(<div className="kv-modal-backdrop kv-modal-backdrop--portal" onMouseDown={e => {
    if (e.target === e.currentTarget && !busy) onCancel()
  }}>
    <div ref={panel} className="kv kv-modal tl-delete-dialog" role="alertdialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={descriptionId} aria-busy={busy} tabIndex={-1}
      onKeyDown={e => {
        if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); if (!busy) onCancel() }
        if (e.key === 'Tab') {
          const buttons = Array.from(panel.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') || [])
          const first = buttons[0], last = buttons[buttons.length - 1]
          if (!first) { e.preventDefault(); panel.current?.focus() }
          else if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus() }
          else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus() }
        }
      }}>
      <h3 id={titleId}>{count === 1 ? '删除任务？' : `删除 ${count} 个任务？`}</h3>
      {name && <p className="tl-delete-name">{name}</p>}
      <p id={descriptionId}>任务记录、专属素材副本和生成文件将永久删除，无法撤销。</p>
      {error && <p className="tl-delete-error" role="alert">{error}</p>}
      <div className="tl-delete-actions">
        <Button ref={cancel} disabled={busy} onClick={onCancel}>取消</Button>
        <Button variant="danger" disabled={busy} onClick={onConfirm}>{busy ? '正在删除…' : '确认删除'}</Button>
      </div>
    </div>
  </div>, document.body)
}
