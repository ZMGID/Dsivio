import { Handle, Position, type Node, type NodeProps } from '@xyflow/react'
import { useT } from '../../../components/i18n'
import { paletteEntry } from './workflowCatalog'
import type { WorkflowNodeKind, WorkflowPortKind } from './workflowModel'

export type GenerationRfNode = Node<{ kind: WorkflowNodeKind; title: string }, 'gen'>

function portLabel(kind: WorkflowPortKind, t: ReturnType<typeof useT>): string {
  if (kind === 'image') return t.wfPortImage
  if (kind === 'text') return t.wfPortText
  if (kind === 'video') return t.wfPortVideo
  return t.wfPortMesh
}

export function WorkflowNodeCard({ data, selected }: NodeProps<GenerationRfNode>) {
  const t = useT()
  const entry = paletteEntry(data.kind)
  return (
    <article className={`workbench-gen-node${selected ? ' is-selected' : ''}`}>
      <header className="workbench-gen-node-head">
        <h3>{data.title}</h3>
        <span>{t.wfIdle}</span>
      </header>
      {entry ? <p className="workbench-gen-node-hint">{entry.hint(t)}</p> : null}
      {entry && entry.outputs.length > 0 ? (
        <dl className="workbench-gen-node-ports">
          <dt>{t.wfOutput}</dt>
          {entry.outputs.map((port) => (
            <dd key={port.id}>{portLabel(port.kind, t)}</dd>
          ))}
        </dl>
      ) : null}
      {entry?.inputs.map((port, index) => (
        <Handle
          key={`in-${port.id}`}
          id={port.id}
          type="target"
          position={Position.Left}
          style={{ top: 28 + index * 18 }}
          className={`workbench-gen-handle workbench-gen-handle--${port.kind}`}
        />
      ))}
      {entry?.outputs.map((port, index) => (
        <Handle
          key={`out-${port.id}`}
          id={port.id}
          type="source"
          position={Position.Right}
          style={{ top: 28 + index * 18 }}
          className={`workbench-gen-handle workbench-gen-handle--${port.kind}`}
        />
      ))}
    </article>
  )
}
