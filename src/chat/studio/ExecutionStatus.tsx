import { useEffect, useState, type ReactNode } from 'react'
import './executionStatus.css'

export function ExecutionStatus({ active, title, detail, children }: {
  active: boolean; title: string; detail: string; children?: ReactNode
}) {
  const [seconds, setSeconds] = useState(0)
  useEffect(() => {
    setSeconds(0)
    if (!active) return
    const started = Date.now()
    const timer = window.setInterval(() => setSeconds(Math.floor((Date.now() - started) / 1000)), 1000)
    return () => window.clearInterval(timer)
  }, [active])
  const elapsed = `${Math.floor(seconds / 60).toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`
  return <section className="studio-execution" aria-label="执行状态" aria-busy={active}>
    <span className="studio-execution-dot" aria-hidden="true" />
    <div className="studio-execution-copy">
      <strong role="status">{title}</strong>
      <span title={detail}>{detail}</span>
    </div>
    {active && <time aria-label={`已等待 ${seconds} 秒`}>{elapsed}</time>}
    {detail && <details className="studio-execution-details">
      <summary>详情</summary>
      <div><strong>{title}</strong><p>{detail}</p></div>
    </details>}
    {children}
  </section>
}
