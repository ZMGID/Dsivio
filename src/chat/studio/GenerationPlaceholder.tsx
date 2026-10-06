import { Loader2 } from 'lucide-react'

/** Keep the result canvas stable while the provider prepares its first output. */
export function GenerationPlaceholder({ ratio, label }: { ratio?: string; label: string }) {
  const parts = ratio?.split(/[:/]/).map(Number)
  const aspect = parts?.length === 2 && parts.every(value => Number.isFinite(value) && value > 0)
    ? Math.max(0.2, Math.min(5, parts[0] / parts[1])) : 1
  return <div role="status" aria-label={label} className="mx-auto flex w-full max-w-lg flex-col items-center justify-center gap-3 rounded-xl border border-[var(--theme-surface-border)] bg-[var(--theme-surface-soft)] text-[var(--text-muted)]"
    style={{ aspectRatio: aspect }}><Loader2 className="animate-spin motion-reduce:animate-none" size={24} /><span>{label}</span></div>
}
