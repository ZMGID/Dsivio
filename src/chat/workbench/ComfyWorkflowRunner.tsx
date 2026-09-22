import { useEffect, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, type ModelProvider } from '../../api/tauri'
import { Button } from '../../components/Button'
import { useLang } from '../../components/i18n'
import type { ComfyTask, ComfyTaskStatus, ComfyWorkflow } from '../../generated/comfyui'
import { FieldBlock, Input, TextArea } from '../../settings/public/controls'

const activeStatuses: ComfyTaskStatus[] = ['submitting', 'queued', 'running']
const labels: Record<ComfyTaskStatus, [string, string]> = {
  submitting: ['正在提交', 'Submitting'], queued: ['排队中', 'Queued'], running: ['生成中', 'Running'],
  succeeded: ['已完成', 'Completed'], failed: ['失败', 'Failed'], uncertain: ['提交待核查', 'Submission needs checking'], download_pending: ['下载待恢复', 'Download pending'],
}
const imageData = (file: File) => new Promise<string>((resolve, reject) => {
  const reader = new FileReader()
  reader.onload = () => resolve(String(reader.result))
  reader.onerror = () => reject(new Error('读取图片失败'))
  reader.readAsDataURL(file)
})

/** The remote server owns execution; local task receipts survive page changes and restarts. */
export function ComfyWorkflowRunner({ provider, workflow }: { provider: ModelProvider; workflow: ComfyWorkflow }) {
  const zh = useLang() === 'zh'
  const [values, setValues] = useState<Record<string, string>>({})
  const [files, setFiles] = useState<Record<string, string>>({})
  const [tasks, setTasks] = useState<ComfyTask[]>([])
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [reload, setReload] = useState(0)
  useEffect(() => {
    let active = true
    let timer: ReturnType<typeof setTimeout> | undefined
    async function poll() {
      try {
        const loaded = await api.listComfyTasks(provider.id, workflow.id)
        if (!active) return
        setTasks(loaded)
        for (const task of loaded.filter(task => activeStatuses.includes(task.status) || task.status === 'download_pending')) {
          const refreshed = await api.refreshComfyTask(task.id)
          if (!active) return
          setTasks(current => current.map(item => item.id === task.id ? refreshed : item))
        }
        if (active && loaded.some(task => activeStatuses.includes(task.status))) timer = setTimeout(() => void poll(), 3500)
      } catch (failure) { if (active) setError(String(failure)) }
      finally { if (active) setLoading(false) }
    }
    setLoading(true); setError(''); void poll()
    return () => { active = false; if (timer) clearTimeout(timer) }
  }, [provider.id, workflow.id, reload])

  async function submit() {
    if (pending.current) return
    pending.current = true; setBusy(true); setError('')
    try {
      const inputs = Object.fromEntries(workflow.inputs.flatMap(binding => {
        const key = `${binding.nodeId}:${binding.input}`
        const value = values[key]
        if (value === undefined) return []
        if (binding.kind === 'number' && (!value.trim() || !Number.isFinite(Number(value)))) throw new Error(`${binding.label}: ${zh ? '请输入有效数字' : 'Enter a valid number'}`)
        return [[key, binding.kind === 'number' ? Number(value) : value]]
      }))
      const task = await api.submitComfyWorkflow(provider.id, workflow.id, inputs)
      setTasks(current => [task, ...current.filter(item => item.id !== task.id)])
      setReload(value => value + 1)
    } catch (failure) { setError(String(failure)) }
    finally { pending.current = false; setBusy(false) }
  }
  return <div className="flex min-w-0 flex-col gap-4">
    <section className="workbench-card-block">
      <h2 className="workbench-card-block-title [overflow-wrap:anywhere]">{workflow.name}</h2>
      <p className="workbench-page-sub">{zh ? '由你的 ComfyUI 服务执行。未修改的参数沿用工作流原值。' : 'Runs on your ComfyUI server. Unchanged inputs keep their workflow defaults.'}</p>
      <fieldset disabled={busy}>{workflow.inputs.map(binding => {
        const key = `${binding.nodeId}:${binding.input}`
        const original = String(workflow.graph[binding.nodeId]?.inputs[binding.input] ?? '')
        return <FieldBlock key={key} label={binding.label}>
          {binding.kind === 'image' ? <>
            <Input aria-label={binding.label} type="file" accept="image/png,image/jpeg,image/webp" value="" onChange={() => {}} disabled={busy} onInput={event => {
              const file = event.currentTarget.files?.[0]
              if (!file || pending.current) return
              if (file.size > 30 * 1024 * 1024) { setError(zh ? '参考图不能超过 30 MB' : 'Reference images must be under 30 MB'); return }
              pending.current = true; setBusy(true); setError('')
              void imageData(file).then(data => { setValues(v => ({ ...v, [key]: data })); setFiles(v => ({ ...v, [key]: file.name })) }).catch(failure => setError(String(failure))).finally(() => { pending.current = false; setBusy(false) })
            }} />
            <p className="workbench-page-sub [overflow-wrap:anywhere]">{files[key] || `${zh ? '沿用服务中的文件' : 'Existing server file'}: ${original}`}</p>
          </> : binding.kind === 'number' ? <Input aria-label={binding.label} type="number" value={values[key] ?? original} disabled={busy} onChange={value => setValues(current => ({ ...current, [key]: value }))} /> :
            <label className="block"><span className="sr-only">{binding.label}</span><TextArea value={values[key] ?? original} onChange={value => setValues(current => ({ ...current, [key]: value }))} rows={3} /></label>}
        </FieldBlock>
      })}</fieldset>
      <div className="flex flex-wrap gap-2 pt-3"><Button variant="primary" disabled={busy || loading} onClick={() => void submit()}>{busy ? (zh ? '正在处理…' : 'Working…') : (zh ? '开始生成' : 'Generate')}</Button></div>
    </section>
    <section className="workbench-card-block">
      <div className="flex min-w-0 items-center justify-between gap-4"><h2 className="workbench-card-block-title">{zh ? '生成任务' : 'Generation tasks'}</h2><Button size="sm" disabled={loading || busy} onClick={() => setReload(value => value + 1)}>{loading ? (zh ? '更新中…' : 'Updating…') : (zh ? '刷新任务' : 'Refresh')}</Button></div>
      {!tasks.length && <p className="workbench-page-sub">{loading ? (zh ? '正在加载任务…' : 'Loading tasks…') : (zh ? '还没有生成任务。' : 'No tasks yet.')}</p>}
      <ul className="divide-y divide-border">{tasks.map(task => <li key={task.id} className="flex min-w-0 flex-col gap-3 py-4">
        <div className="flex min-w-0 flex-wrap justify-between gap-2"><span className="kv-row-label">{new Date(task.createdAt).toLocaleString()}</span><span className="kv-row-label" role="status">{labels[task.status][zh ? 0 : 1]}</span></div>
        {task.promptId && <p className="kv-row-desc [overflow-wrap:anywhere]">{zh ? '任务编号' : 'Task ID'}: {task.promptId}</p>}
        {task.error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{task.error}</p>}
        <div className="grid min-w-0 grid-cols-1 gap-3 sm:grid-cols-2">{task.outputs.filter(output => output.localPath).map((output, index) => <div key={index} className="min-w-0">
          {task.kind === 'image' || /\.gif$/i.test(output.filename) ? <img className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.localPath!)} alt={workflow.name} loading="lazy" /> : <video className="h-auto max-w-full rounded-lg" src={convertFileSrc(output.localPath!)} controls preload="metadata" />}
          <Button size="sm" className="mt-2" onClick={() => void api.openLocalFile(output.localPath!).catch(failure => setError(String(failure)))}>{zh ? '打开文件' : 'Open file'}</Button>
        </div>)}</div>
      </li>)}</ul>
    </section>
    {error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{error}</p>}
  </div>
}
