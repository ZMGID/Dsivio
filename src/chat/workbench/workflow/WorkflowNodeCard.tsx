import type { WorkflowStatus } from '../../../generated/generationWorkflow'
import { runStatus } from './workflowModel'
import { Handle, Position, type Node, type NodeProps } from '@xyflow/react'
import { useT } from '../../../components/i18n'
import { paletteEntry } from './workflowCatalog'
import { nodeConfig } from './workflowConfig'
import type { WorkflowNode } from './workflowModel'

export type GenerationRfNode = Node<{ node: WorkflowNode; problems: string[]; status?: WorkflowStatus }, 'gen'>
export function WorkflowNodeCard({ data, selected }: NodeProps<GenerationRfNode>) {
  const t = useT(), entry = paletteEntry(data.node.kind), config = nodeConfig(data.node)
  const summary = config.type === 'prompt' || config.type === 'text' ? config.text : config.type === 'understand' ? config.instruction : config.type === 'generate' ? `${config.model?.model || '未选模型'} · ${Object.values(config.options).join(' · ')}` : config.type === 'assets' ? config.assets.map(a => a.name).join('、') : ''
  return <article className={`workbench-gen-node${selected ? ' is-selected' : ''} ${data.status ? `is-${data.status}` : ''}`}>
    <header className="workbench-gen-node-head"><h3 title={data.node.title}>{data.node.title}</h3><span>{data.status ? runStatus[data.status] : config.type === 'placeholder' ? '待完善' : '草稿'}</span></header>
    {entry && <p className="workbench-gen-node-hint">{entry.hint(t)}</p>}
    {summary && <p className="workbench-gen-node-summary" title={summary}>{summary}</p>}
    {config.type !== 'placeholder' && data.problems.length > 0 && <p className="workbench-gen-node-summary" title={data.problems.join('；')}>待补充：{data.problems.join('；')}</p>}
    {(['inputs', 'outputs'] as const).map(direction => <div key={direction} className="workbench-gen-node-ports">
      {entry?.[direction].map(port => <div className="workbench-gen-port" key={port.id}>
        {/* Handle size is ReactFlow geometry: use its style API so lazy vendor CSS cannot shrink the hit target. */}
        <Handle
          style={{ width: 12, height: 12 }}
          aria-label={`${port.label}${direction === 'inputs' ? '输入' : '输出'}`}
          id={port.id}
          type={direction === 'inputs' ? 'target' : 'source'}
          position={direction === 'inputs' ? Position.Left : Position.Right}
          className={`workbench-gen-handle workbench-gen-handle--${port.kind}`}
        />
        <span>{direction === 'inputs' ? '入' : '出'} · {port.label}{direction === 'inputs' ? port.required ? ' *' : '（可选）' : ''}</span>
      </div>)}
    </div>)}
  </article>
}
