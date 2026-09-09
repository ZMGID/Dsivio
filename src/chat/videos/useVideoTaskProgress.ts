import { useEffect, useRef, useState } from 'react'
import { api } from '../../api/tauri'
import type { VideoTask } from './types'

// Query existing jobs in bounded batches. This path never creates or submits jobs.
export function useVideoTaskProgress(tasks: VideoTask[], enabled: boolean, onUpdate: (task: VideoTask) => void) {
  const current = useRef({ tasks, onUpdate })
  current.current = { tasks, onUpdate }
  const cursor = useRef(0)
  const inFlight = useRef(false)
  const [error, setError] = useState('')
  useEffect(() => {
    if (!enabled) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout>
    const poll = async () => {
      if (stopped) return
      if (inFlight.current || document.visibilityState === 'hidden') {
        timer = setTimeout(() => void poll(), 8000)
        return
      }
      inFlight.current = true
      const running = current.current.tasks.filter(t => t.status === 'running' && t.remote?.id)
      const batch = Array.from({ length: Math.min(3, running.length) }, (_, i) => running[(cursor.current + i) % running.length])
      cursor.current += batch.length
      const results = await Promise.allSettled(batch.map(async t => {
        const latest = await api.videoStudioTask('get', { id: t.id })
        if (stopped) return
        const result = latest.status === 'running' && latest.remote?.id
          ? await api.videoStudioTask('poll', { id: latest.id, revision: latest.revision }) : latest
        if (!stopped) current.current.onUpdate(result)
      }))
      inFlight.current = false
      if (stopped) return
      setError(results.some(r => r.status === 'rejected') ? '部分视频进度暂时无法更新，已有任务已保留；可打开任务查看并重试。' : '')
      timer = setTimeout(() => void poll(), 8000)
    }
    timer = setTimeout(() => void poll(), 1000)
    return () => { stopped = true; clearTimeout(timer) }
  }, [enabled])
  return error
}
