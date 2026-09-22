import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../api/tauri'
import type { MediaRequest, MediaTask, MediaTaskFilter } from '../../generated/mediaGeneration'
import type { WorkbenchSubpageId } from './registry'

const POLL_MS = 2500

/** 工作台页面写在任务上的来源标记；同一页的历史靠它列出来，与选了哪个模型无关。 */
export function workbenchOrigin(page: WorkbenchSubpageId): string {
  return `workbench/${page}`
}

export interface MediaGeneration {
  tasks: MediaTask[]
  /** 首次列表尚未返回。 */
  loading: boolean
  /** 正在提交或读取素材；期间不接受第二次提交。 */
  busy: boolean
  error: string
  setError: (message: string) => void
  /** 提交一次生成。失败时写入 `error` 并返回 undefined；作用域切换后迟到的结果被丢弃。 */
  submit: (request: MediaRequest | (() => Promise<MediaRequest>)) => Promise<MediaTask | undefined>
  /** 对已保存的任务继续查询／下载，不会重新提交。 */
  resume: (id: string) => Promise<void>
  refresh: () => void
  /** 读取本地素材等前置步骤期间占住提交按钮。 */
  hold: (value: boolean) => void
}

/**
 * 媒体生成流程的唯一前端负责人：列出某个作用域的任务、轮询运行中的任务、提交与恢复。
 *
 * `filter` 决定列哪些任务：工作台页面按 `origin` 列自己的历史；聊天里的工作室按 provider+model 列。
 * 传 `null` 表示还没有可用作用域（比如没选模型），此时不请求。
 */
export function useMediaGeneration(filter: Partial<MediaTaskFilter> | null): MediaGeneration {
  const scopeKey = filter ? JSON.stringify(filter) : ''
  const [tasks, setTasks] = useState<MediaTask[]>([])
  const [loading, setLoading] = useState(Boolean(filter))
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [reload, setReload] = useState(0)
  const pending = useRef(false)
  const mounted = useRef(true)
  const currentScope = useRef(scopeKey)
  currentScope.current = scopeKey

  useEffect(() => {
    mounted.current = true
    return () => { mounted.current = false }
  }, [])

  useEffect(() => {
    if (!scopeKey) { setTasks([]); setLoading(false); return }
    const scope = JSON.parse(scopeKey) as Partial<MediaTaskFilter>
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    async function poll() {
      try {
        const loaded = await api.listMediaTasks(scope)
        if (!active) return
        setTasks(loaded)
        if (loaded.some((task) => task.status === 'running')) timer = setTimeout(() => void poll(), POLL_MS)
      } catch (failure) {
        if (active) setError(String(failure))
      } finally {
        if (active) setLoading(false)
      }
    }
    setLoading(true)
    setError('')
    void poll()
    return () => { active = false; if (timer) clearTimeout(timer) }
  }, [scopeKey, reload])

  const refresh = useCallback(() => setReload((value) => value + 1), [])

  const hold = useCallback((value: boolean) => {
    pending.current = value
    if (mounted.current) setBusy(value)
  }, [])

  const submit = useCallback(async (request: MediaRequest | (() => Promise<MediaRequest>)) => {
    if (pending.current) return undefined
    const submittedScope = currentScope.current
    pending.current = true
    setBusy(true)
    setError('')
    try {
      const prepared = typeof request === 'function' ? await request() : request
      if (!mounted.current || currentScope.current !== submittedScope) return undefined
      const task = await api.startMediaGeneration(prepared)
      if (!mounted.current || currentScope.current !== submittedScope) return undefined
      setTasks((current) => [task, ...current.filter((item) => item.id !== task.id)])
      setReload((value) => value + 1)
      return task
    } catch (failure) {
      if (mounted.current && currentScope.current === submittedScope) setError(String(failure))
      return undefined
    } finally {
      pending.current = false
      if (mounted.current) setBusy(false)
    }
  }, [])

  const resume = useCallback(async (id: string) => {
    try {
      await api.getMediaTask(id, true)
      if (mounted.current) setReload((value) => value + 1)
    } catch (failure) {
      if (mounted.current) setError(String(failure))
    }
  }, [])

  return { tasks, loading, busy, error, setError, submit, resume, refresh, hold }
}
