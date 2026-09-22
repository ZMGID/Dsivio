import { useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api } from '../../../api/tauri'
import { Button } from '../../../components/Button'
import { Select } from '../../../settings/public/controls'
import { runStatus } from './workflowModel'
import type { useWorkflowRun } from './useWorkflowRun'


export function WorkflowRunPanel({ execution, onClose }: { execution: ReturnType<typeof useWorkflowRun>; onClose: () => void }) {
  const { runs, selected: run } = execution
  const [fileError, setFileError] = useState('')
  return <aside className="workbench-flow-config custom-scrollbar" aria-label="运行记录">
    <div className="workbench-flow-config-head"><h3>运行记录</h3><Button size="sm" onClick={onClose}>关闭</Button></div>
    {execution.loading ? <p>正在读取…</p> : !runs.length ? <p>运行后会显示每个节点的状态和结果。离开页面不会中止运行。</p> : null}
    {runs.length > 0 && <Select ariaLabel="选择运行记录" value={run?.id ?? ''} onChange={execution.select} options={runs.map(r => ({ value: r.id, label: `${new Date(r.createdAt).toLocaleString()} · ${runStatus[r.status]}` }))} />}
    {fileError && <p role="alert">{fileError}</p>}
    {run && <>
      <p>{run.workflow.name} · {runStatus[run.status]}</p>
      <p className="workbench-page-sub">此记录使用运行时的配置快照。继续运行会保留已完成节点；修改草稿后请重新运行。</p>
      {run.error && <p role="alert">{run.error}</p>}
      {['failed', 'interrupted', 'cancelled'].includes(run.status) && <Button size="sm" disabled={execution.busy} onClick={() => void execution.resume(run.id)}>继续此运行</Button>}
      {run.nodes.map(node => {
        const definition = run.workflow.nodes.find(n => n.id === node.nodeId)
        return <section className="workbench-flow-run-node" key={node.nodeId}>
          <div className="workbench-flow-config-head"><strong>{definition?.title}</strong><span>{runStatus[node.status]}</span></div>
          {node.error && <p role="alert">{node.error}</p>}
          {Object.entries(node.outputs).map(([handle, value]) => <div key={handle} className="workbench-flow-fields">
            {value.text && <pre className="workbench-flow-result-text">{value.text}</pre>}
            {value.files.map(path => <div className="workbench-flow-asset" key={path}>
              {handle === 'image' && <img src={convertFileSrc(path)} alt={`${definition?.title}的输出`} />}
              {handle === 'video' && <video src={convertFileSrc(path)} controls preload="metadata" />}
              <span title={path}>{path.split(/[/\\]/).pop()}</span>
              <Button size="sm" onClick={() => void api.openLocalFile(path).catch(e => setFileError(String(e)))}>打开文件</Button>
            </div>)}
          </div>)}
        </section>
      })}
    </>}
  </aside>
}
