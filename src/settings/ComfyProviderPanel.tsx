import { useRef, useState } from 'react'
import { api, type ModelProvider } from '../api/tauri'
import { Button } from '../components/Button'
import type { Lang } from '../components/i18n'
import type { ComfyWorkflow, ComfyInputKind } from '../generated/comfyui'
import { FieldBlock, Input, Select } from './public/controls'
import { SettingsGroup } from './components'

function readGraph(text: string): ComfyWorkflow['graph'] {
  const raw = JSON.parse(text)
  if (!raw || typeof raw !== 'object' || Array.isArray(raw) || raw.nodes || raw.links) throw new Error('请在 ComfyUI 中导出 API 格式 JSON，不能使用画布 JSON。')
  const graph = raw.prompt && typeof raw.prompt === 'object' ? raw.prompt : raw
  if (!Object.keys(graph).length || Object.values(graph).some(node => !node || typeof node !== 'object' || !('class_type' in node) || !('inputs' in node))) throw new Error('工作流缺少 class_type 或 inputs。')
  return graph
}

export function ComfyProviderPanel({ provider, lang, onUpdateProvider }: {
  provider: ModelProvider
  lang: Lang
  onUpdateProvider: (id: string, updates: Partial<ModelProvider>) => void
}) {
  const zh = lang === 'zh'
  const [draft, setDraft] = useState<ComfyWorkflow | null>(null)
  const [busy, setBusy] = useState(false)
  const pending = useRef(false)
  const [error, setError] = useState('')
  const [message, setMessage] = useState('')
  const [feedbackAt, setFeedbackAt] = useState<'connection' | 'workflow'>('workflow')
  const workflows = provider.request.comfy?.workflows ?? []
  const fields = draft ? Object.entries(draft.graph).flatMap(([nodeId, node]) => Object.entries(node.inputs).filter(([, value]) => typeof value === 'number' || typeof value === 'string').map(([input, value]) => ({ key: `${nodeId}:${input}`, nodeId, input, value, label: `${node._meta?.title || node.class_type} · ${input} (${nodeId})` }))) : []
  async function run(action: () => Promise<void>, at: 'connection' | 'workflow' = 'workflow') {
    if (pending.current) return
    pending.current = true; setBusy(true); setError(''); setMessage(''); setFeedbackAt(at)
    try { await action() } catch (failure) { setError(String(failure)) } finally { pending.current = false; setBusy(false) }
  }
  async function importFile(file?: File) {
    if (!file) return
    await run(async () => {
      if (file.size > 5 * 1024 * 1024) throw new Error(zh ? '工作流文件不能超过 5 MB' : 'Workflow files must be under 5 MB')
      const graph = readGraph(await file.text())
      const outputs = Object.entries(graph).filter(([, node]) => /^(SaveImage|SaveVideo|SaveAnimatedWEBP|VHS_VideoCombine)$/.test(node.class_type)).map(([id]) => id)
      setDraft({ id: `comfy:${crypto.randomUUID()}`, name: file.name.replace(/\.json$/i, ''), kind: 'image', graph, inputs: [], outputNodes: outputs.slice(0, 1) })
    })
  }
  function updateWorkflows(next: ComfyWorkflow[]) {
    onUpdateProvider(provider.id, { request: { ...provider.request, comfy: { workflows: next } }, enabledModels: next.map(w => w.id), availableModels: next.map(w => w.id) })
  }
  return <>
    <SettingsGroup title="ComfyUI">
      <FieldBlock label={zh ? '服务地址' : 'Server address'} description={zh ? '同机填写 http://127.0.0.1:8188；其他电脑填写局域网地址。' : 'Use http://127.0.0.1:8188 on this computer, or your server’s LAN address.'}>
        <Input aria-label={zh ? 'ComfyUI 服务地址' : 'ComfyUI server address'} value={provider.baseUrl} disabled={busy} onChange={baseUrl => { setMessage(''); onUpdateProvider(provider.id, { baseUrl }) }} mono />
      </FieldBlock>
      <div className="flex flex-wrap gap-2 py-2">
        <Button size="sm" disabled={busy} onClick={() => void run(async () => {
          const result = await api.testComfyConnection(provider.baseUrl, draft)
          if (result.missingNodes.length) throw new Error(`${zh ? '服务缺少节点' : 'Missing nodes'}: ${result.missingNodes.join(', ')}`)
          setMessage(`${zh ? '连接成功' : 'Connected'}${result.devices.length ? ` · ${result.devices.join(', ')}` : ''}${draft ? (zh ? ' · 工作流节点齐全' : ' · All workflow nodes available') : ''}`)
        }, 'connection')}>{busy ? (zh ? '处理中…' : 'Working…') : (draft ? (zh ? '测试连接与节点' : 'Test connection and nodes') : (zh ? '测试连接' : 'Test connection'))}</Button>
      </div>
      {feedbackAt === 'connection' && error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{error}</p>}
      {feedbackAt === 'connection' && message && <p className="kv-row-desc [overflow-wrap:anywhere]" role="status">{message}</p>}
    </SettingsGroup>
    <SettingsGroup title={zh ? '工作流' : 'Workflows'}>
      {workflows.length === 0 && !draft && <p className="kv-row-desc py-3">{zh ? '导入已有工作流，然后加入「媒体创作」中的图片或视频池。' : 'Import a workflow, then add it to an image or video pool in Media creation.'}</p>}
      <ul className="divide-y divide-border">{workflows.map(workflow => <li key={workflow.id} className="flex min-w-0 items-center gap-4 py-3">
        <div className="min-w-0 flex-1 [overflow-wrap:anywhere]"><div className="kv-row-label">{workflow.name}</div><p className="kv-row-desc">ComfyUI · {workflow.kind === 'image' ? (zh ? '图片' : 'Image') : (zh ? '视频' : 'Video')} · {workflow.inputs.length} {zh ? '个输入参数' : 'inputs'}</p></div>
        <Button size="sm" disabled={busy || Boolean(draft)} onClick={() => { setDraft(structuredClone(workflow)); setError(''); setMessage('') }}>{zh ? '配置' : 'Configure'}</Button>
      </li>)}</ul>
      {!draft && <FieldBlock label={zh ? '导入 API 工作流 JSON' : 'Import API workflow JSON'}>
        <Input aria-label={zh ? '导入 API 工作流 JSON' : 'Import API workflow JSON'} type="file" accept=".json,application/json" value="" onChange={() => {}} disabled={busy} onInput={event => { const file = event.currentTarget.files?.[0]; void importFile(file) }} />
      </FieldBlock>}
      {draft && <div className="flex min-w-0 flex-col gap-3 py-3">
        <FieldBlock label={zh ? '工作流名称' : 'Workflow name'}><Input aria-label={zh ? '工作流名称' : 'Workflow name'} value={draft.name} disabled={busy} onChange={name => setDraft({ ...draft, name })} /></FieldBlock>
        <FieldBlock label={zh ? '生成类型' : 'Output type'}><Select className="w-full" ariaLabel={zh ? '生成类型' : 'Output type'} value={draft.kind} disabled={busy} options={[{ value: 'image', label: zh ? '图片' : 'Image' }, { value: 'video', label: zh ? '视频' : 'Video' }]} onChange={kind => setDraft({ ...draft, kind: kind as ComfyWorkflow['kind'] })} /></FieldBlock>
        <FieldBlock label={zh ? '输出节点' : 'Output node'} description={zh ? '选择保存最终图片或视频的节点。' : 'Choose the node that saves your final image or video.'}>
          <Select className="w-full" ariaLabel={zh ? '输出节点' : 'Output node'} value={draft.outputNodes[0] || ''} disabled={busy} options={[{ value: '', label: zh ? '选择输出节点' : 'Choose output node' }, ...Object.entries(draft.graph).map(([id, node]) => ({ value: id, label: `${node._meta?.title || node.class_type} (${id})` }))]} onChange={id => setDraft({ ...draft, outputNodes: id ? [id] : [] })} />
        </FieldBlock>
        <div className="kv-row-label">{zh ? 'Workbench 可填写的参数' : 'Inputs shown in Workbench'}</div>
        <p className="kv-row-desc">{zh ? '添加提示词、参考图、尺寸或种子。未添加的参数保留工作流原值。' : 'Expose prompts, reference images, dimensions or seeds. Other values stay as imported.'}</p>
        {draft.inputs.map((binding, index) => <div key={index} className="flex min-w-0 flex-col gap-2 border-b border-border pb-3">
          <FieldBlock label={zh ? '绑定节点参数' : 'Node input'}><Select className="w-full" ariaLabel={`${zh ? '绑定参数' : 'Input binding'} ${index + 1}`} value={`${binding.nodeId}:${binding.input}`} disabled={busy} options={fields.filter(f => f.key === `${binding.nodeId}:${binding.input}` || !draft.inputs.some(b => `${b.nodeId}:${b.input}` === f.key)).map(f => ({ value: f.key, label: f.label }))} onChange={key => {
            const field = fields.find(f => f.key === key)!
            setDraft({ ...draft, inputs: draft.inputs.map((b, i) => i === index ? { nodeId: field.nodeId, input: field.input, label: field.label, kind: typeof field.value === 'number' ? 'number' : 'text' } : b) })
          }} /></FieldBlock>
          <FieldBlock label={zh ? '显示名称' : 'Label'}><Input aria-label={`${zh ? '参数名称' : 'Input label'} ${index + 1}`} value={binding.label} disabled={busy} onChange={label => setDraft({ ...draft, inputs: draft.inputs.map((b, i) => i === index ? { ...b, label } : b) })} /></FieldBlock>
          {binding.kind !== 'number' && <FieldBlock label={zh ? '输入方式' : 'Input type'}><Select className="w-full" ariaLabel={`${zh ? '输入方式' : 'Input type'} ${index + 1}`} value={binding.kind} disabled={busy} options={[{ value: 'text', label: zh ? '文字' : 'Text' }, { value: 'image', label: zh ? '上传参考图' : 'Reference image upload' }]} onChange={kind => setDraft({ ...draft, inputs: draft.inputs.map((b, i) => i === index ? { ...b, kind: kind as ComfyInputKind } : b) })} /></FieldBlock>}
          <Button className="self-start" size="sm" variant="ghost" disabled={busy} onClick={() => setDraft({ ...draft, inputs: draft.inputs.filter((_, i) => i !== index) })}>{zh ? '移除参数' : 'Remove input'}</Button>
        </div>)}
        <div className="flex flex-wrap gap-2">
          <Button size="sm" disabled={busy || fields.length === draft.inputs.length} onClick={() => {
            const field = fields.find(f => !draft.inputs.some(b => `${b.nodeId}:${b.input}` === f.key))
            if (field) setDraft({ ...draft, inputs: [...draft.inputs, { nodeId: field.nodeId, input: field.input, label: field.label, kind: typeof field.value === 'number' ? 'number' : 'text' }] })
          }}>{zh ? '添加参数' : 'Add input'}</Button>
          <Button size="sm" variant="primary" disabled={busy || !draft.name.trim() || !draft.outputNodes.length} onClick={() => void run(async () => {
            await api.validateComfyWorkflow(draft)
            updateWorkflows(workflows.some(w => w.id === draft.id) ? workflows.map(w => w.id === draft.id ? draft : w) : [...workflows, draft])
            setDraft(null); setMessage(zh ? '工作流已加入配置，请到「媒体创作」中勾选使用。' : 'Workflow configured. Select it in Media creation to use it.')
          })}>{zh ? '完成配置' : 'Done'}</Button>
          <Button size="sm" disabled={busy} onClick={() => { setDraft(null); setError('') }}>{zh ? '取消' : 'Cancel'}</Button>
        </div>
      </div>}
    </SettingsGroup>
    {feedbackAt === 'workflow' && error && <p className="kv-row-desc [overflow-wrap:anywhere]" role="alert">{error}</p>}
    {feedbackAt === 'workflow' && message && <p className="kv-row-desc [overflow-wrap:anywhere]" role="status">{message}</p>}
  </>
}
