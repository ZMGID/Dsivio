import { convertFileSrc } from '@tauri-apps/api/core'
import { useRef, useState } from 'react'
import { api } from '../../api/tauri'
import { Button } from '../../components/Button'
import { useLang } from '../../components/i18n'
import type { MediaCancelResult, MediaTask } from '../../generated/mediaGeneration'
import type { MediaGeneration } from './useMediaGeneration'

/** 一条生成记录：时间、状态、错误、恢复按钮和输出预览。 */
export function MediaTaskRow({
  task,
  alt,
  onResume,
  onCancel,
  onError,
}: {
  task: MediaTask
  alt: string
  onResume: (id: string) => void
  onCancel: (id: string) => Promise<MediaCancelResult | undefined>
  onError: (message: string) => void
}) {
  const zh = useLang() === 'zh'
  const [cancelling, setCancelling] = useState(false)
  const [cancellation, setCancellation] = useState<MediaCancelResult | null>(null)
  const pending = useRef(false)
  const kind = { image: zh ? '图片' : 'Image', video: zh ? '视频' : 'Video', speech: zh ? '语音' : 'Speech', transcribe: zh ? '转写' : 'Transcription' }[task.kind]
  const status = {
    running: task.kind === 'transcribe' ? (zh ? '转写中' : 'Transcribing') : task.kind === 'speech' ? (zh ? '合成中' : 'Synthesizing') : (zh ? '生成中' : 'Generating'),
    succeeded: zh ? '完成' : 'Completed',
    failed: zh ? '失败' : 'Failed',
    cancelled: zh ? '已取消' : 'Cancelled',
  }[task.status]
  const outcome = cancellation?.outcome || task.cancellation?.outcome
  const outcomeLabels = {
    confirmed: zh ? '已确认取消' : 'Cancellation confirmed',
    requested: zh ? '已请求取消，尚未确认停止' : 'Cancellation requested; stopping is not confirmed',
    unsupported: zh ? '不支持取消，任务仍可继续完成' : 'Cancellation unsupported; the task may still complete',
    'too-late': zh ? '已无法取消，保留任务结果' : 'Too late to cancel; task results are retained',
  }
  const chargeLabels = { no: zh ? '未收费' : 'Not charged', maybe: zh ? '可能已收费' : 'May have been charged', yes: zh ? '已收费' : 'Charged', unknown: zh ? '费用未知' : 'Charges unknown' }
  async function cancel() {
    if (pending.current) return
    pending.current = true; setCancelling(true)
    try { const result = await onCancel(task.id); if (result) setCancellation(result) }
    finally { pending.current = false; setCancelling(false) }
  }
  return (
    <li className="flex min-w-0 flex-col gap-3 py-4">
      <div className="flex min-w-0 flex-wrap justify-between gap-2">
        <span className="kv-row-label">{kind} · {new Date(task.createdAt).toLocaleString()}</span>
        <span className="kv-row-label" role="status">{status}</span>
      </div>
      {task.prompt ? <p className="workbench-page-sub workbench-page-sub--flush line-clamp-2 [overflow-wrap:anywhere]">{task.prompt}</p> : null}
      {task.error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{task.error}</p>}
      {outcome && <p className="kv-row-desc" role="status">{outcomeLabels[outcome]}{cancellation ? ` · ${chargeLabels[cancellation.charged]}` : ''}</p>}
      {task.status === 'running' && <Button size="sm" className="self-start" disabled={cancelling} onClick={() => void cancel()}>{cancelling ? (zh ? '正在请求取消…' : 'Requesting cancellation…') : (zh ? '取消任务' : 'Cancel task')}</Button>}
      {task.canResume && (
        <Button size="sm" className="self-start" onClick={() => onResume(task.id)}>
          {zh ? '恢复查询／下载' : 'Resume query / download'}
        </Button>
      )}
      {task.outputs.length > 0 && (
        <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">
          {task.outputs.map((output) => (
            <div key={output.path} className="min-w-0">
              {output.mime.startsWith('image/')
                ? <img className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.path)} alt={alt} loading="lazy" />
                : output.mime.startsWith('video/')
                  ? <video className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.path)} controls preload="metadata" />
                  : output.mime.startsWith('audio/')
                    ? <audio className="max-w-full" src={convertFileSrc(output.path)} controls preload="metadata" aria-label={alt} />
                    : <p className="kv-row-desc [overflow-wrap:anywhere]">{output.mime} · {output.path}</p>}
              <Button size="sm" className="mt-2" onClick={() => void api.openLocalFile(output.path).catch((failure) => onError(String(failure)))}>
                {zh ? '打开文件' : 'Open file'}
              </Button>
            </div>
          ))}
        </div>
      )}
    </li>
  )
}

/**
 * 生成记录列表：`useMediaGeneration` 的默认展示。页面只决定外框和文案。
 */
export function MediaTaskList({
  generation,
  alt,
  title,
  className = 'workbench-card-block',
  bare = false,
}: {
  generation: MediaGeneration
  alt: string
  title?: string
  className?: string
  /** 只渲染列表本体，不带标题栏与外框（放进页面自己的卡片时用）。 */
  bare?: boolean
}) {
  const zh = useLang() === 'zh'
  const { tasks, loading, busy, refresh, resume, cancel, setError } = generation
  const body = (
    <>
      {!tasks.length && (
        <p className="workbench-page-sub">{loading ? (zh ? '正在加载…' : 'Loading…') : (zh ? '还没有生成记录。' : 'No generations yet.')}</p>
      )}
      <ul className="divide-y divide-border">
        {tasks.map((task) => <MediaTaskRow key={task.id} task={task} alt={alt} onResume={(id) => void resume(id)} onCancel={cancel} onError={setError} />)}
      </ul>
    </>
  )
  if (bare) return body
  return (
    <section className={className}>
      <div className="flex min-w-0 items-center justify-between gap-4">
        <h2 className="workbench-card-block-title">{title ?? (zh ? '生成记录' : 'Generations')}</h2>
        <Button size="sm" disabled={loading || busy} onClick={refresh}>{zh ? '刷新' : 'Refresh'}</Button>
      </div>
      {body}
    </section>
  )
}
