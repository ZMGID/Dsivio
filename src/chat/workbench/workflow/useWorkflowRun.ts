import { useCallback, useEffect, useRef, useState } from 'react'
import { api, isTauriRuntime } from '../../../api/tauri'
import type { WorkflowRun } from '../../../generated/generationWorkflow'
import type { GenerationWorkflow } from './workflowModel'
import { checkWorkflow } from './workflowValidation'

/** Subscribes to durable backend runs. Leaving the canvas only stops polling, never execution. */
export function useWorkflowRun(workflowId: string) {
  const [runs, setRuns] = useState<WorkflowRun[]>([])
  const [selectedId, select] = useState('')
  const [error, setError] = useState(''), [loadError, setLoadError] = useState('')
  const [pending, setPending] = useState(false)
  const [loading, setLoading] = useState(isTauriRuntime)
  const alive = useRef(false), locked = useRef(false), version = useRef(0)
  const refresh = useCallback(async () => {
    if (!isTauriRuntime()) return
    const stamp = version.current
    try {
      const next = await api.listWorkflowRuns(workflowId)
      if (alive.current && stamp === version.current) { setRuns(next); setLoadError('') }
    } catch (failure) { if (alive.current && stamp === version.current) setLoadError(`运行记录读取失败：${String(failure)}`) }
    finally { if (alive.current) setLoading(false) }
  }, [workflowId])
  useEffect(() => {
    alive.current = true
    const revision = version
    void refresh()
    return () => { alive.current = false; revision.current++ }
  }, [refresh])
  const active = runs.find(run => run.status === 'running')
  const activeId = active?.id
  useEffect(() => {
    if (!activeId) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => { await refresh(); if (!cancelled) timer = setTimeout(() => void poll(), 1500) }
    timer = setTimeout(() => void poll(), 1500)
    return () => { cancelled = true; clearTimeout(timer) }
  }, [activeId, refresh])
  async function action(work: () => Promise<WorkflowRun | void>) {
    if (locked.current) return
    locked.current = true; version.current++; setPending(true); setError('')
    try {
      if (!isTauriRuntime()) throw new Error('请在桌面应用中运行工作流；浏览器可编辑和导出配置。')
      const result = await work()
      if (alive.current && result) {
        setRuns(current => [result, ...current.filter(r => r.id !== result.id)])
        select(result.id)
      }
    } catch (failure) { if (alive.current) setError(String(failure)) }
    finally { locked.current = false; version.current++; if (alive.current) setPending(false) }
  }
  async function start(flow: GenerationWorkflow) {
    if (active || loading) return
    await action(async () => {
      const issues = await checkWorkflow(flow)
      if (issues.length) throw new Error(issues.join('；'))
      return api.startWorkflowRun(flow)
    })
  }
  return { runs, selected: runs.find(r => r.id === selectedId) ?? runs[0], select, error: error || loadError, pending, loading,
    busy: pending || !!active || loading, active, refresh, start,
    resume: (id: string) => { if (!active && !loading) return action(() => api.resumeWorkflowRun(id)) },
    cancel: () => action(async () => { if (active) await api.cancelWorkflowRun(active.id) }),
  }
}
