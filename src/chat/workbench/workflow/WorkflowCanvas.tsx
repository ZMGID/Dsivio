import { checkWorkflow } from './workflowValidation'
import { useEffect, useRef, useState } from 'react'
import { applyNodeChanges, Background, BackgroundVariant, Controls, ReactFlow, ReactFlowProvider, type ReactFlowInstance, type Edge, type Connection, type EdgeChange, type NodeChange } from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { ArrowLeft, PanelLeft, Undo2, Redo2, Copy, Trash2, LayoutGrid, Plus, Search, Type, Image, Video, Box, Wrench, Download, Upload, Play, History, CheckCheck, Square, X } from 'lucide-react'
import { Button, IconButton } from '../../../components/Button'
import { useT } from '../../../components/i18n'
import { Input } from '../../../settings/public/controls'
import { setHash } from '../../chatRoutes'
import { PALETTE_GROUPS, WORKFLOW_PALETTE, paletteEntry, type PaletteGroupId } from './workflowCatalog'
import { WorkflowNodeCard, type GenerationRfNode } from './WorkflowNodeCard'
import { workbenchHash } from '../workbenchPages'
import { defaultConfig } from './workflowConfig'
import { arrangeNodes, connectionProblem, duplicateNodes, nodeProblems, removeSelection } from './workflowGraph'
import { useWorkflowRun } from './useWorkflowRun'
import { WorkflowRunPanel } from './WorkflowRunPanel'
import { WorkflowNodePanel } from './WorkflowNodePanel'
import { isTextEditing, useWorkflowEditor } from './useWorkflowEditor'
import type { GenerationWorkflow, WorkflowEdge, WorkflowNodeKind } from './workflowModel'

const nodeTypes = { gen: WorkflowNodeCard }
const groupIcons = { input: Upload, tool: Wrench, text: Type, image: Image, video: Video, mesh: Box, output: Download }
function CanvasInner({ initial }: { initial: GenerationWorkflow }) {
  const t = useT(), editor = useWorkflowEditor(initial), { flow, change } = editor
  const execution = useWorkflowRun(initial.id)
  const [showRuns, setShowRuns] = useState(false), [showPalette, setShowPalette] = useState(true)
  const canvas = useRef<HTMLDivElement>(null), instance = useRef<ReactFlowInstance<GenerationRfNode> | null>(null)
  const [checking, setChecking] = useState(false)
  const checkLock = useRef(false)
  const mounted = useRef(true), snapshot = useRef(flow)
  snapshot.current = flow
  useEffect(() => { mounted.current = true; return () => { mounted.current = false } }, [])
  async function check() {
    if (checkLock.current) return
    checkLock.current = true; setChecking(true)
    const issues = await checkWorkflow(flow)
    checkLock.current = false
    if (!mounted.current) return
    setChecking(false)
    if (snapshot.current !== flow) { setNotice('检查期间配置发生变化，请重新检查。'); return }
    setNotice(issues.length ? issues.join('；') : '配置检查通过，可以运行。')
  }
  const [group, setGroup] = useState<PaletteGroupId>('input')
  const [nodeQuery, setNodeQuery] = useState('')
  const paletteItems = WORKFLOW_PALETTE.filter(item => nodeQuery.trim() ? `${item.label(t)} ${item.hint(t)}`.toLowerCase().includes(nodeQuery.trim().toLowerCase()) : item.group === group)
  const [selectedNodes, setSelectedNodes] = useState<string[]>([]), [selectedEdges, setSelectedEdges] = useState<string[]>([])
  // ReactFlow measurements belong to this mounted canvas, never to saved business snapshots.
  const [measured, setMeasured] = useState<Record<string, GenerationRfNode['measured']>>({})
  const dragActive = useRef(false)
  const connectionNotice = useRef('')
  const [notice, setNotice] = useState('点击节点配置 · 拖动端口连线 · 拖动空白框选 · 滚轮平移')
  const nodes: GenerationRfNode[] = flow.nodes.map(node => ({ id: node.id, type: 'gen', position: node.position, measured: measured[node.id], selected: selectedNodes.includes(node.id), data: { node, problems: nodeProblems(flow, node), status: execution.selected?.nodes.find(n => n.nodeId === node.id)?.status } }))
  const edges: Edge[] = flow.edges.map(edge => ({ ...edge, selected: selectedEdges.includes(edge.id) }))
  const selected = flow.nodes.find(node => selectedNodes.length === 1 && node.id === selectedNodes[0])
  const focusedNodeId = selected?.id
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (focusedNodeId && !showRuns) void instance.current?.fitView({ nodes: [{ id: focusedNodeId }], padding: 0.25, maxZoom: 1 })
    })
    return () => cancelAnimationFrame(frame)
  }, [focusedNodeId, showRuns, showPalette])
  function onNodesChange(changes: NodeChange<GenerationRfNode>[]) {
    const dimensions = changes.filter(c => c.type === 'dimensions')
    if (dimensions.length) setMeasured(current => Object.fromEntries(
      applyNodeChanges(dimensions, nodes.map(node => ({ ...node, measured: current[node.id] }))).map(node => [node.id, node.measured]),
    ))
    setSelectedNodes(current => {
      let next = current
      for (const item of changes) if (item.type === 'select') next = item.selected ? [...new Set([...next, item.id])] : next.filter(id => id !== item.id)
      return next
    })
    const removed = changes.filter(c => c.type === 'remove').map(c => c.id)
    const positions = changes.filter(c => c.type === 'position')
    if (!removed.length && !positions.length) return
    const dragging = positions.some(p => p.dragging)
    const isDrag = dragging || dragActive.current
    dragActive.current = dragging
    const moveKey = isDrag ? 'drag:selection' : `move:${positions.map(p => p.id).sort().join(',')}`
    change(current => {
      const next = removeSelection(current, removed, [])
      return { ...next, nodes: next.nodes.map(node => { const move = positions.find(p => p.id === node.id); return move?.position ? { ...node, position: move.position } : node }) }
    }, removed.length ? '' : moveKey, !dragging)
    if (isDrag && !dragging) editor.endGroup()
  }
  function onEdgesChange(changes: EdgeChange[]) {
    setSelectedEdges(current => {
      let next = current
      for (const item of changes) if (item.type === 'select') next = item.selected ? [...new Set([...next, item.id])] : next.filter(id => id !== item.id)
      return next
    })
    const removed = changes.filter(c => c.type === 'remove').map(c => c.id)
    if (removed.length) change(current => removeSelection(current, [], removed))
  }
  function connect(connection: Connection) {
    const edge: WorkflowEdge = { ...connection, id: crypto.randomUUID(), sourceHandle: connection.sourceHandle ?? undefined, targetHandle: connection.targetHandle ?? undefined }
    const problem = connectionProblem(flow, edge)
    if (problem) { setNotice(problem); return }
    change(current => ({ ...current, edges: [...current.edges, edge] })); setNotice('已连接；上游供值时保留本地配置。')
  }
  function addNode(kind: WorkflowNodeKind, point?: { x: number; y: number }) {
    if (defaultConfig(kind).type === 'placeholder') return
    const bounds = canvas.current?.getBoundingClientRect()
    const center = point ?? (bounds ? { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 3 } : null)
    let position = center && instance.current ? instance.current.screenToFlowPosition(center) : { x: 80, y: 80 }
    while (flow.nodes.some(n => Math.abs(n.position.x - position.x) < 24 && Math.abs(n.position.y - position.y) < 24)) position = { x: position.x + 48, y: position.y + 48 }
    const node = { id: crypto.randomUUID(), kind, title: paletteEntry(kind)!.label(t), position, config: defaultConfig(kind) }
    change(current => ({ ...current, nodes: [...current.nodes, node] })); setSelectedNodes([node.id]); setSelectedEdges([]); setShowRuns(false)
  }
  const duplicate = () => {
    const next = duplicateNodes(flow, selectedNodes)
    change(next); setSelectedNodes(next.nodes.filter(n => !flow.nodes.some(old => old.id === n.id)).map(n => n.id)); setSelectedEdges([])
  }
  function arrange() { change(arrangeNodes(flow)); requestAnimationFrame(() => void instance.current?.fitView({ padding: 0.15, maxZoom: 1 })) }
  const closePanel = () => { setSelectedNodes([]); setShowRuns(false); canvas.current?.focus() }
  const remove = () => { change(current => removeSelection(current, selectedNodes, selectedEdges)); setSelectedNodes([]); setSelectedEdges([]) }
  return <div className="workbench-flow-shell" onKeyDown={event => {
    if (event.key === 'Escape') {
      if (!event.defaultPrevented && !document.querySelector('[role="listbox"], [role="dialog"]')) closePanel()
      return
    }
    if (isTextEditing(event.target)) return
    const modifier = event.metaKey || event.ctrlKey
    if (modifier && event.key.toLowerCase() === 'z') { event.preventDefault(); if (event.shiftKey) editor.redo(); else editor.undo() }
    else if (modifier && event.key.toLowerCase() === 'y') { event.preventDefault(); editor.redo() }
    else if (modifier && event.key.toLowerCase() === 'd') { event.preventDefault(); duplicate() }
    else if (event.key === 'Delete' || event.key === 'Backspace') { event.preventDefault(); remove() }
  }} onKeyUp={event => {
    if (!isTextEditing(event.target) && event.key.startsWith('Arrow')) editor.endGroup()
  }}>
    <header className="workbench-flow-bar">
      <div className="workbench-flow-identity">
        <Button onClick={() => { if (editor.save()) setHash(workbenchHash('workflows')) }}><ArrowLeft size={14} />{t.wfBack}</Button>
        <div className="workbench-flow-title"><Input value={flow.name} onChange={name => change(current => ({ ...current, name }), 'name')} onBlur={editor.endGroup} aria-label={t.workbenchNavWorkflows} /></div>
      </div>
      <div className="workbench-flow-run-actions" role="group" aria-label="运行操作">
        <Button disabled={checking} onClick={() => void check()}><CheckCheck size={14} />{checking ? '正在检查…' : '检查配置'}</Button>
        <Button aria-pressed={showRuns} onClick={() => { setShowRuns(v => !v); setSelectedNodes([]) }}><History size={14} />运行记录</Button>
        {execution.active ? <Button disabled={execution.pending} onClick={() => void execution.cancel()}><Square size={14} />{execution.pending ? '正在停止…' : '停止运行'}</Button> : <Button variant="primary" disabled={execution.busy || checking} onClick={() => { if (editor.save()) { setShowRuns(true); setSelectedNodes([]); void execution.start(structuredClone(flow)) } }}><Play size={14} />{execution.pending ? '正在启动…' : t.wfRun}</Button>}
      </div>
    </header>
    <div className="workbench-flow-tools">
      <Button aria-pressed={showPalette} onClick={() => setShowPalette(v => !v)}><PanelLeft size={14} />{showPalette ? '收起节点库' : '展开节点库'}</Button>
      <div className="workbench-flow-tool-group" role="group" aria-label="编辑历史">
        <IconButton label="撤销" disabled={!editor.canUndo} onClick={editor.undo}><Undo2 size={15} /></IconButton>
        <IconButton label="重做" disabled={!editor.canRedo} onClick={editor.redo}><Redo2 size={15} /></IconButton>
      </div>
      <div className="workbench-flow-tool-group" role="group" aria-label="节点操作">
        <Button disabled={!selectedNodes.length} onClick={duplicate}><Copy size={14} />复制节点</Button>
        <Button disabled={!selectedNodes.length && !selectedEdges.length} onClick={remove}><Trash2 size={14} />删除所选</Button>
        <Button disabled={!flow.nodes.length} onClick={arrange}><LayoutGrid size={14} />自动排列</Button>
      </div>
      <span className="workbench-flow-count">{flow.nodes.length} 个节点 · {flow.edges.length} 条连线{execution.active ? ' · 后台运行中' : ''}</span>
    </div>
    <div className="workbench-flow-body custom-scrollbar">
      {showPalette && <aside className="workbench-flow-palette" aria-label="节点库">
        <div className="workbench-flow-palette-head"><strong>添加节点</strong><span>点击添加，也可拖入画布</span></div>
        <div className="workbench-flow-palette-search"><Search size={14} /><Input aria-label="搜索节点" placeholder="搜索节点…" value={nodeQuery} onChange={setNodeQuery} />{nodeQuery && <IconButton label="清空节点搜索" onClick={() => setNodeQuery('')}><X size={14} /></IconButton>}</div>
        {/* ui-guard-ignore:raw-primitive — category filters follow the Button.tsx segmented-control exception. */}
        <div className="workbench-flow-palette-rail" aria-label="节点分类">{PALETTE_GROUPS.map(item => {
          const Icon = groupIcons[item.id]
          // ui-guard-ignore:raw-primitive — category filter, per Button.tsx segmented-control contract.
          return <button key={item.id} type="button" className={`workbench-flow-rail${!nodeQuery && group === item.id ? ' is-active' : ''}`} aria-pressed={!nodeQuery && group === item.id} onClick={() => { setGroup(item.id); setNodeQuery('') }}><Icon size={14} />{item.label(t)}</button>
        })}</div>
        <div className="workbench-flow-palette-list custom-scrollbar">
          {paletteItems.length === 0 && <p className="workbench-page-sub">没有匹配的节点，试试其他关键词。</p>}
          {paletteItems.map(item => {
            const unavailable = defaultConfig(item.kind).type === 'placeholder', Icon = groupIcons[item.group]
            // ui-guard-ignore:raw-primitive — draggable palette rows follow the Button.tsx list-item exception.
            return <button key={item.kind} type="button" className="workbench-flow-palette-item" aria-label={`添加${item.label(t)}`} title={unavailable ? '暂未支持运行' : `点击添加${item.label(t)}，或拖入画布`} disabled={unavailable} draggable={!unavailable} onDragStart={event => { event.dataTransfer.setData('application/dsivio-workflow-node', item.kind); event.dataTransfer.effectAllowed = 'copy' }} onClick={() => addNode(item.kind)}>
              <span className="workbench-flow-palette-item-head"><Icon size={16} /><strong>{item.label(t)}</strong><span className="workbench-flow-palette-add">{unavailable ? '未开放' : <><Plus size={12} />添加</>}</span></span>
              <span className="workbench-flow-palette-hint">{item.hint(t)}</span>
            </button>
          })}
        </div>
      </aside>}
      <div ref={canvas} onDragOver={event => { event.preventDefault(); event.dataTransfer.dropEffect = 'copy' }} onDrop={event => {
        event.preventDefault()
        const kind = event.dataTransfer.getData('application/dsivio-workflow-node') as WorkflowNodeKind
        if (paletteEntry(kind)) addNode(kind, { x: event.clientX, y: event.clientY })
      }} className="workbench-flow-canvas" tabIndex={0} aria-label="工作流画布">
        <ReactFlow onInit={api => { instance.current = api }} onNodeClick={() => setShowRuns(false)} panOnScroll selectionOnDrag panOnDrag={[1, 2]} minZoom={0.2} maxZoom={2} nodes={nodes} edges={edges} nodeTypes={nodeTypes} onNodesChange={onNodesChange} onEdgesChange={onEdgesChange} onConnect={connect}
          onConnectStart={() => { connectionNotice.current = '' }}
          isValidConnection={connection => {
            connectionNotice.current = connectionProblem(flow, { ...connection, sourceHandle: connection.sourceHandle ?? undefined, targetHandle: connection.targetHandle ?? undefined }) ?? ''
            return !connectionNotice.current
          }}
          onConnectEnd={(_, state) => { if (state.isValid === false) setNotice(connectionNotice.current || '连接方向无效，请将输出端口连接到输入端口。') }}
          fitViewOptions={{ padding: 0.15, maxZoom: 1 }} onNodeDragStop={() => { editor.save(); editor.endGroup() }} onPaneClick={() => { setSelectedNodes([]); setSelectedEdges([]) }} fitView deleteKeyCode={null} proOptions={{ hideAttribution: true }}>
          <Background variant={BackgroundVariant.Dots} gap={18} size={1} /><Controls showInteractive={false} />
        </ReactFlow>
      </div>
      {showRuns && <div className="workbench-flow-config-wrap"><WorkflowRunPanel execution={execution} onClose={closePanel} /></div>}
      {selected && !showRuns && <div className="workbench-flow-config-wrap" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) editor.endGroup() }}><WorkflowNodePanel key={selected.id} flow={flow} node={selected} onChange={(node, field) => change(current => ({ ...current, nodes: current.nodes.map(n => n.id === node.id ? node : n) }), `${node.id}:${field}`)} onDisconnect={port => change(current => ({ ...current, edges: current.edges.filter(e => !(e.target === selected.id && e.targetHandle === port)) }))} onClose={closePanel} /></div>}
    </div>
    <div className="workbench-flow-log custom-scrollbar">
      {editor.saveError ? <p role="alert">{editor.saveError}<Button size="sm" onClick={() => editor.save()}>重试保存</Button></p> : <span>已保存到本机</span>}
      <p role="status">{notice}</p>
      {execution.error && <p role="alert">{execution.error}<Button size="sm" disabled={execution.pending} onClick={() => void execution.refresh()}>刷新记录</Button></p>}
    </div>
  </div>
}
export function WorkflowCanvas({ flow }: { flow: GenerationWorkflow }) {
  return <ReactFlowProvider><CanvasInner key={flow.id} initial={flow} /></ReactFlowProvider>
}
