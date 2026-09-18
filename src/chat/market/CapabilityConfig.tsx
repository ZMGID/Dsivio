import { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { X } from 'lucide-react'
import { refreshSettings, saveSettingsCached, subscribeSettings } from '../../api/settingsCache'
import type { Lang } from '../../settings/i18n'
import { Button } from '../../components/Button'
import { CapabilityTextEditor } from './CapabilityTextEditor'

export function CapabilityConfig({ lang, open, onClose }: { lang: Lang; open: boolean; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)
  const [value, setValue] = useState('')
  const [ready, setReady] = useState(false)
  const [status, setStatus] = useState('')
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  const draft = useRef('')
  const saved = useRef('')
  const loaded = useRef(false)
  const mounted = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout>>()
  const pending = useRef<Promise<void> | null>(null)
  const zh = lang === 'zh'
  const flush = () => {
    if (pending.current) return pending.current
    if (!loaded.current || draft.current === saved.current) return
    const work = async () => {
      while (draft.current !== saved.current) {
        const text = draft.current
        if (mounted.current) { setStatus(zh ? '正在保存…' : 'Saving…'); setError('') }
        try {
          const settings = await refreshSettings()
          await saveSettingsCached({ ...settings, capabilityConfigText: text })
          saved.current = text
        } catch (e) {
          if (mounted.current) { setError(String(e)); setStatus('') }
          return
        }
      }
      if (mounted.current) setStatus(zh ? '已保存' : 'Saved')
    }
    pending.current = work().finally(() => { pending.current = null })
    return pending.current
  }
  const flushRef = useRef(flush)
  flushRef.current = flush
  useEffect(() => {
    mounted.current = true
    return () => { mounted.current = false; clearTimeout(timer.current); flushRef.current() }
  }, [])
  useEffect(() => {
    if (!open || loaded.current) return
    let disposed = false
    setError('')
    void refreshSettings().then(settings => {
      if (disposed) return
      draft.current = saved.current = settings.capabilityConfigText ?? ''
      loaded.current = true; setValue(draft.current); setReady(true)
    }).catch(e => { if (!disposed) setError(String(e)) })
    return () => { disposed = true }
  }, [open, retry])
  useEffect(() => subscribeSettings(settings => {
    if (!loaded.current || pending.current || draft.current !== saved.current) return
    draft.current = saved.current = settings.capabilityConfigText ?? ''
    setValue(draft.current)
  }), [])
  useEffect(() => {
    if (!open) return
    const element = dialog.current
    const previousFocus = document.activeElement as HTMLElement | null
    element?.showModal()
    return () => { element?.close(); previousFocus?.focus() }
  }, [open])
  const close = async () => {
    clearTimeout(timer.current)
    await flushRef.current()
    if (draft.current === saved.current) onClose()
  }
  if (!open) return null
  return createPortal(<dialog ref={dialog} aria-labelledby="capability-config-title" onCancel={event => { event.preventDefault(); void close() }} className="m-auto max-h-[85vh] w-[min(720px,calc(100vw-32px))] overflow-y-auto rounded-2xl border border-neutral-200 bg-white p-6 text-neutral-900 shadow-2xl backdrop:bg-black/40 dark:border-neutral-700 dark:bg-neutral-900 dark:text-neutral-100">
    <header className="mb-4 flex items-center justify-between gap-4">
      <h2 id="capability-config-title" className="text-lg font-semibold">{zh ? '能力配置' : 'Capabilities'}</h2>
      <Button size="sm" aria-label={zh ? '关闭能力配置' : 'Close capabilities'} onClick={() => void close()}><X size={18} /></Button>
    </header>
    {ready ? <CapabilityTextEditor lang={lang} value={value} onChange={text => {
      draft.current = text; setValue(text); setStatus(zh ? '等待保存…' : 'Unsaved changes'); setError('')
      clearTimeout(timer.current); timer.current = setTimeout(() => flushRef.current(), 500)
    }} /> : !error && <p role="status">{zh ? '正在读取配置…' : 'Loading configuration…'}</p>}
    {status && <p role="status" className="mt-2 text-xs text-neutral-500">{status}</p>}
    {error && <div role="alert" className="mt-3 text-sm"><p>{zh ? '配置读取或保存失败，修改未丢弃。' : 'Could not load or save configuration. Edits are retained.'}</p><p>{error}</p><Button size="sm" onClick={() => { if (ready) flushRef.current(); else setRetry(n => n + 1) }}>{zh ? '重试' : 'Retry'}</Button></div>}
  </dialog>, document.body)
}
