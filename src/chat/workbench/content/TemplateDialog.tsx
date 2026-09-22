import { useEffect, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { X } from 'lucide-react'
import { IconButton } from '../../../components/Button'

/** Template preview/editor shell, using the app's existing modal styles. */
export function TemplateDialog({ title, closeLabel, onClose, children, footer, className = '', busy = false }: {
  title: string
  closeLabel: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  className?: string
  busy?: boolean
}) {
  const overlay = useRef<HTMLDivElement>(null)
  const dialog = useRef<HTMLElement>(null)
  const close = useRef(onClose)
  close.current = onClose
  useEffect(() => {
    const trigger = document.activeElement
    const overflow = document.body.style.overflow
    const siblings = Array.from(document.body.children).filter((node): node is HTMLElement => node instanceof HTMLElement && node !== overlay.current)
    const inert = siblings.map(node => node.inert)
    siblings.forEach(node => { node.inert = true })
    document.body.style.overflow = 'hidden'
    dialog.current?.focus()
    return () => {
      document.body.style.overflow = overflow
      siblings.forEach((node, index) => { node.inert = inert[index] })
      if (trigger instanceof HTMLElement && trigger.isConnected) trigger.focus()
    }
  }, [])
  return createPortal(<div ref={overlay} className="kv-modal-backdrop kv-modal-backdrop--portal" onMouseDown={event => {
    if (event.target === event.currentTarget) close.current()
  }}>
    <section ref={dialog} className={`kv-modal content-template-dialog ${className}`} role="dialog" aria-modal="true" aria-busy={busy} aria-label={title} tabIndex={-1} onKeyDown={event => {
      if (event.key === 'Escape') { event.stopPropagation(); close.current() }
      if (event.key !== 'Tab') return
      const targets = Array.from(dialog.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href], [tabindex="0"]') ?? []).filter(element => !element.closest('fieldset:disabled'))
      const first = targets[0], last = targets[targets.length - 1]
      if (!first) { event.preventDefault(); return }
      if (event.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { event.preventDefault(); last.focus() }
      else if (!event.shiftKey && (document.activeElement === last || document.activeElement === dialog.current)) { event.preventDefault(); first.focus() }
    }}>
      <header className="content-template-dialog-head"><h2>{title}</h2><IconButton disabled={busy} label={closeLabel} onClick={() => close.current()}><X size={18} /></IconButton></header>
      <div className="content-template-dialog-body custom-scrollbar">{children}</div>
      {footer && <footer className="content-template-dialog-actions">{footer}</footer>}
    </section>
  </div>, document.body)
}
