import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { api, isTauriRuntime } from '../../../api/tauri'
import type { WorkflowRun } from '../../../generated/generationWorkflow'
import type { GenerationWorkflow } from './workflowModel'
import { checkWorkflow } from './workflowValidation'

/** Follows durable backend runs. Leaving the canvas drops updates; it never cancels execution. */
export function useWorkflowRun(workflowId: string) {
  const [runs, setRuns] = useState<WorkflowRun[]>([])
  const [selectedId, select] = useState('')
  const [error, setError] = useState(''), [loadError, setLoadError] = useState('')
  const [pending, setPending] = useState(false)
  const [loading, setLoading] = useState(isTauriRuntime)
  const alive = useRef(false), locked = useRef(false), version = useRef(0)
  const readSerial = useRef(0)
  const observedRuns = useRef(new Map<string, number>())
  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) return
    const stamp = version.current
    const request = ++readSerial.current
    try {
      const next = await api.listWorkflowRuns(workflowId)
      if (alive.current && stamp === version.current && request === readSerial.current) {
        const revision = ++version.current
        observedRuns.current = new Map(next.map(run => [run.id, revision]))
        setRuns(next); setLoadError('')
      }
    } catch (failure) { if (alive.current && stamp === version.current && request === readSerial.current) setLoadError(`运行记录读取失败：${String(failure)}`) }
    finally { if (alive.current) setLoading(false) }
  }, [workflowId])
  const mergeRun = useCallback((run: WorkflowRun) => {
    if (!alive.current || run.workflow.id !== workflowId) return
    version.current++
    observedRuns.current.set(run.id, version.current)
    setRuns(current => {
      const index = current.findIndex(item => item.id === run.id)
      if (index < 0) return [run, ...current]
      const next = current.slice()
      next[index] = run
      return next
    })
  }, [workflowId])
  useEffect(() => {
    alive.current = true
    const revision = version
    void refresh()
    return () => { alive.current = false; revision.current++ }
  }, [refresh])
  useEffect(() => {
    if (!isTauriRuntime()) return
    let unlisten: (() => void) | undefined
    let stopped = false
    void listen<WorkflowRun>('workflow-run-updated', event => {
      if (stopped) return
      mergeRun(event.payload)
    }).then(stop => { if (stopped) stop(); else unlisten = stop })
    return () => { stopped = true; unlisten?.() }
  }, [mergeRun])
  const active = runs.find(run => run.status === 'running')
  const activeId = active?.id
  useEffect(() => {
    if (!activeId) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => { await refresh(); if (!cancelled) timer = setTimeout(() => void poll(), 5000) }
    timer = setTimeout(() => void poll(), 5000)
    return () => { cancelled = true; clearTimeout(timer) }
  }, [activeId, refresh])
  async function action(work: () => Promise<WorkflowRun | void>) {
    if (locked.current) return
    locked.current = true; version.current++; setPending(true); setError('')
    const stamp = version.current
    let succeeded = false
    try {
      if (!isTauriRuntime()) throw new Error('请在桌面应用中运行工作流；浏览器可编辑和导出配置。')
      const result = await work()
      if (alive.current && result) {
        if ((observedRuns.current.get(result.id) ?? 0) <= stamp) {
          setRuns(current => [result, ...current.filter(item => item.id !== result.id)])
        }
        select(result.id)
      }
      succeeded = true
    } catch (failure) { if (alive.current) setError(String(failure)) }
    finally { locked.current = false; version.current++; if (alive.current) setPending(false) }
    if (succeeded && alive.current) await refresh()
  }
  async function start(currentFlow: () => GenerationWorkflow) {
    if (active || loading) return
    await action(async () => {
      const checked = currentFlow()
      const issues = await checkWorkflow(checked)
      if (issues.length) throw new Error(issues.join('；'))
      const latest = currentFlow()
      if (latest !== checked) throw new Error('检查期间配置发生变化，请重新运行')
      return api.startWorkflowRun(latest)
    })
  }
  return { runs, selected: runs.find(run => run.id === selectedId) ?? runs[0], select, error: error || loadError, pending, loading,
    busy: pending || !!active || loading, active, refresh, start,
    resume: (id: string) => { if (!active && !loading) return action(() => api.resumeWorkflowRun(id)) },
    cancel: () => action(async () => { if (active) await api.cancelWorkflowRun(active.id) }),
  }
}
