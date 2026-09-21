import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type Edge,
  type EdgeChange,
  type NodeChange,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { ArrowLeft } from 'lucide-react'
import { Button } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input } from '../../../settings/public/controls'
import { setHash } from '../../chatRoutes'
import { PALETTE_GROUPS, WORKFLOW_PALETTE, canConnectPorts, paletteEntry, type PaletteGroupId } from './workflowCatalog'
import { WorkflowNodeCard, type GenerationRfNode } from './WorkflowNodeCard'
import { workbenchHash } from '../workbenchPages'
import { workflowStore } from './workflowStore'
import type { GenerationWorkflow, WorkflowNode } from './workflowModel'

const nodeTypes = { gen: WorkflowNodeCard }

function toRfNodes(nodes: WorkflowNode[]): GenerationRfNode[] {
  return nodes.map((node) => ({
    id: node.id,
    type: 'gen' as const,
    position: node.position,
    data: { kind: node.kind, title: node.title },
  }))
}

function toRfEdges(flow: GenerationWorkflow): Edge[] {
  return flow.edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    sourceHandle: edge.sourceHandle,
    targetHandle: edge.targetHandle,
  }))
}

function graphToFlow(flow: GenerationWorkflow, nodes: GenerationRfNode[], edges: Edge[]): GenerationWorkflow {
  const byId = new Map(flow.nodes.map((node) => [node.id, node]))
  return {
    ...flow,
    nodes: nodes.map((node) => ({
      id: node.id,
      kind: node.data.kind,
      title: byId.get(node.id)?.title ?? node.data.title,
      position: node.position,
      note: byId.get(node.id)?.note,
    })),
    edges: edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: edge.sourceHandle ?? undefined,
      targetHandle: edge.targetHandle ?? undefined,
    })),
  }
}

function CanvasInner({ initial }: { initial: GenerationWorkflow }) {
  const t = useT()
  const flowRef = useRef(initial)
  const [name, setName] = useState(initial.name)
  const [group, setGroup] = useState<PaletteGroupId>('input')
  const [nodes, setNodes] = useState<GenerationRfNode[]>(() => toRfNodes(initial.nodes))
  const [edges, setEdges] = useState<Edge[]>(() => toRfEdges(initial))
  const [notice, setNotice] = useState(`${t.wfLogLoaded}${initial.name}`)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    flowRef.current = initial
    setName(initial.name)
    setNodes(toRfNodes(initial.nodes))
    setEdges(toRfEdges(initial))
    setNotice(`${t.wfLogLoaded}${initial.name}`)
  }, [initial, t])

  const scheduleSave = useCallback((next: GenerationWorkflow) => {
    flowRef.current = next
    if (saveTimer.current) window.clearTimeout(saveTimer.current)
    saveTimer.current = window.setTimeout(() => {
      workflowStore.save(next)
    }, 250)
  }, [])

  useEffect(() => {
    scheduleSave(graphToFlow(flowRef.current, nodes, edges))
  }, [edges, nodes, scheduleSave])

  useEffect(() => () => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current)
    workflowStore.save(flowRef.current)
  }, [])

  const onNodesChange = useCallback((changes: NodeChange<GenerationRfNode>[]) => {
    setNodes((current) => applyNodeChanges(changes, current))
  }, [])

  const onEdgesChange = useCallback((changes: EdgeChange[]) => {
    setEdges((current) => applyEdgeChanges(changes, current))
  }, [])

  const onConnect = useCallback((connection: Connection) => {
    const source = nodes.find((node) => node.id === connection.source)
    const target = nodes.find((node) => node.id === connection.target)
    const from = paletteEntry(source?.data.kind ?? 'image.upload')?.outputs.find((port) => port.id === (connection.sourceHandle ?? 'image'))
    const to = paletteEntry(target?.data.kind ?? 'image.download')?.inputs.find((port) => port.id === (connection.targetHandle ?? 'image'))
    if (!from || !to || !canConnectPorts(from.kind, to.kind)) return
    setEdges((current) => addEdge({ ...connection, id: crypto.randomUUID() }, current))
  }, [nodes])

  const addNode = useCallback((kind: GenerationRfNode['data']['kind']) => {
    const entry = paletteEntry(kind)
    if (!entry) return
    const node: WorkflowNode = {
      id: crypto.randomUUID(),
      kind,
      title: entry.label(t),
      position: { x: 80 + nodes.length * 36, y: 80 + nodes.length * 28 },
    }
    setNodes((current) => [...current, {
      id: node.id,
      type: 'gen',
      position: node.position,
      data: { kind, title: node.title },
    }])
  }, [nodes.length, t])

  const rename = useCallback((value: string) => {
    setName(value)
    scheduleSave({ ...flowRef.current, name: value })
  }, [scheduleSave])

  const palette = useMemo(() => WORKFLOW_PALETTE.filter((item) => item.group === group), [group])

  return (
    <div className="workbench-flow-shell">
      <header className="workbench-flow-bar">
        <Button size="sm" variant="ghost" onClick={() => setHash(workbenchHash('workflows'))}>
          <ArrowLeft size={14} />
          {t.wfBack}
        </Button>
        <div className="workbench-flow-title">
          <Input value={name} onChange={rename} aria-label={t.workbenchNavWorkflows} />
        </div>
        <span className="workbench-page-sub workbench-page-sub--flush">{t.wfDraft}</span>
        <Button size="sm" variant="primary" onClick={() => setNotice(t.wfRunSoon)}>{t.wfRun}</Button>
      </header>
      <p className="workbench-flow-log" role="status">{`${t.wfRunLog} · ${notice}`}</p>
      <div className="workbench-flow-body">
        <aside className="workbench-flow-palette">
          <div className="workbench-flow-palette-rail">
            {PALETTE_GROUPS.map((item) => (
              <button
                key={item.id}
                type="button"
                className={`workbench-flow-rail${group === item.id ? ' is-active' : ''}`}
                onClick={() => setGroup(item.id)}
              >
                {item.label(t)}
              </button>
            ))}
          </div>
          <div className="workbench-flow-palette-list custom-scrollbar">
            <p className="workbench-page-sub">{t.wfPaletteHint}</p>
            {palette.map((item) => (
              <button
                key={item.kind}
                type="button"
                className="workbench-flow-palette-item"
                onClick={() => addNode(item.kind)}
              >
                <span>{item.label(t)}</span>
                <span className="workbench-page-sub">{item.hint(t)}</span>
              </button>
            ))}
          </div>
        </aside>
        <div className="workbench-flow-canvas">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            fitView
            deleteKeyCode={['Backspace', 'Delete']}
            proOptions={{ hideAttribution: true }}
          >
            <Background variant={BackgroundVariant.Dots} gap={18} size={1} />
            <Controls showInteractive={false} />
            <MiniMap pannable zoomable />
          </ReactFlow>
        </div>
      </div>
    </div>
  )
}

export function WorkflowCanvas({ flow }: { flow: GenerationWorkflow }) {
  return (
    <ReactFlowProvider>
      <CanvasInner initial={flow} />
    </ReactFlowProvider>
  )
}
