import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { api } from '../../api/tauri'
import type { AiTaskRequest, AiTaskResult } from '../../generated/aiTask'

export type AiTaskDelta = { taskId: string; delta: string }

export type AiTaskOptions = Pick<AiTaskRequest, 'mode' | 'prompt'> & Partial<Omit<AiTaskRequest, 'taskId' | 'mode' | 'prompt'>> & {
  onDelta?: (delta: string) => void
}

/**
 * One headless AI call, including optional async input preparation. Busy calls
 * return null. `cancel` and unmount stop submission and drop late results.
 */
export function useAiTask() {
  const currentId = useRef<string | null>(null)
  const onDelta = useRef<((delta: string) => void) | undefined>(undefined)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [partial, setPartial] = useState('')

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let stopped = false
    void listen<AiTaskDelta>('ai-task-delta', (event) => {
      if (event.payload.taskId !== currentId.current) return
      setPartial((text) => text + event.payload.delta)
      onDelta.current?.(event.payload.delta)
    }).then((stop) => {
      if (stopped) stop()
      else unlisten = stop
    })
    return () => {
      stopped = true
      unlisten?.()
      const id = currentId.current
      currentId.current = null
      if (id) void api.cancelAiTask(id)
    }
  }, [])

  const cancel = useCallback(() => {
    const id = currentId.current
    currentId.current = null
    if (id) void api.cancelAiTask(id)
    setBusy(false)
  }, [])

  const run = useCallback(async (input: AiTaskOptions | (() => Promise<AiTaskOptions>)): Promise<string | null> => {
    if (currentId.current) return null
    const taskId = crypto.randomUUID()
    currentId.current = taskId
    setBusy(true)
    setError(null)
    setPartial('')
    try {
      const options = typeof input === 'function' ? await input() : input
      if (currentId.current !== taskId) return null
      onDelta.current = options.onDelta
      const result: AiTaskResult = await api.runAiTask({
        taskId,
        mode: options.mode,
        system: options.system ?? null,
        prompt: options.prompt,
        images: options.images ?? [],
        ...(options.videos?.length ? { videos: options.videos } : {}),
        tools: options.tools ?? [],
        slot: options.slot ?? 'chat',
        providerId: options.providerId ?? null,
        model: options.model ?? null,
        cwd: options.cwd ?? null,
        timeoutSecs: options.timeoutSecs ?? null,
        stream: options.stream ?? false,
      })
      if (currentId.current !== taskId) return null
      return result.text
    } catch (err) {
      if (currentId.current !== taskId) return null
      setError(err instanceof Error ? err.message : String(err))
      return null
    } finally {
      if (currentId.current === taskId) {
        currentId.current = null
        setBusy(false)
      }
    }
  }, [])

  return { run, cancel, busy, error, partial }
}
