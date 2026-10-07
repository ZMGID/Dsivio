/** @vitest-environment jsdom */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ComponentProps } from 'react'
import type { ReactFlow } from '@xyflow/react'
import { adoptUserNodes } from '@xyflow/system'
import type { WorkflowRun, WorkflowStatus } from '../../../generated/generationWorkflow'
import { WorkflowCanvas } from './WorkflowCanvas'
import { blankWorkflow, type GenerationWorkflow } from './workflowModel'
import { workflowStore } from './workflowStore'

const executionState = vi.hoisted(() => ({
  runs: [] as WorkflowRun[],
  selected: undefined as WorkflowRun | undefined,
  select: vi.fn(),
  error: '',
  pending: false,
  loading: false,
  busy: false,
  active: undefined as WorkflowRun | undefined,
  refresh: vi.fn(async () => {}),
  start: vi.fn(),
  resume: vi.fn(),
  cancel: vi.fn(),
}))
vi.mock('./useWorkflowRun', () => ({ useWorkflowRun: () => executionState }))

let canvasProps: ComponentProps<typeof ReactFlow>
vi.mock('@xyflow/react', async () => ({
  ...await vi.importActual<typeof import('@xyflow/react')>('@xyflow/react'),
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) => children,
  Background: () => null, Controls: () => null, MiniMap: () => null,
  BackgroundVariant: { Dots: 'dots' }, Position: { Left: 'left', Right: 'right' }, Handle: () => null,
  // Exercise the real editor through ReactFlow's public callback boundary; no browser simulation.
  ReactFlow: (props: ComponentProps<typeof ReactFlow>) => {
    canvasProps = props
    const { nodes = [], edges = [], onNodesChange, onEdgesChange, onConnect, onNodeDragStop } = props
    return <div>
    {nodes.map(node => <button key={node.id} onClick={() => onNodesChange?.(nodes.map(n => ({ type: 'select', id: n.id, selected: n.id === node.id })))}>{String((node.data.node as { title: string }).title)}</button>)}
    <button onClick={() => { const node = nodes[0]; if (!node) return; onNodesChange?.([{ type: 'position', id: node.id, position: { x: 100, y: 200 }, dragging: true }]) }}>drag</button>
    <button onClick={() => { const node = nodes[0]; if (!node) return; onNodesChange?.([{ type: 'position', id: node.id, position: { x: 150, y: 250 }, dragging: false }]); onNodeDragStop?.({} as never, node, [node]) }}>drop</button>
    <button onClick={() => onConnect?.({ source: nodes[0].id, target: nodes[1].id, sourceHandle: 'text', targetHandle: 'text' })}>connect</button>
    {edges.map(edge => <button key={edge.id} onClick={() => onEdgesChange?.([{ type: 'select', id: edge.id, selected: true }])}>edge</button>)}
  </div> },
}))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn().mockResolvedValue({ providers: [], workbenchMedia: {} }), subscribeSettings: () => () => {} }))
vi.mock('../../../components/i18n', async () => {
  const actual = await vi.importActual<typeof import('../../../components/i18n')>('../../../components/i18n')
  return { ...actual, useT: () => actual.i18n.zh }
})
const draft = (): GenerationWorkflow => ({ ...blankWorkflow('测试编排'), nodes: [
  { id: 'prompt', kind: 'prompt.input' as const, title: '输入提示', position: { x: 0, y: 0 }, config: { type: 'prompt' as const, text: '原文' } },
  { id: 'output', kind: 'text.preview' as const, title: '文本输出', position: { x: 300, y: 0 }, config: { type: 'output' as const } },
] })
function resetExecution() {
  executionState.runs = []
  executionState.selected = undefined
  executionState.active = undefined
  executionState.error = ''
  executionState.pending = false
  executionState.loading = false
  executionState.busy = false
  executionState.start.mockClear()
  executionState.resume.mockClear()
  executionState.cancel.mockClear()
}
function recordedRun(flow: ReturnType<typeof draft>, status: WorkflowStatus, nodeStatus: WorkflowStatus, mediaTaskId: string | null): WorkflowRun {
  return { id: status, workflow: flow, status, createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-01T00:00:00.000Z', error: status === 'failed' ? '失败' : null, nodes: [{ nodeId: 'prompt', status: nodeStatus, error: null, startedAt: null, finishedAt: null, mediaTaskId, outputs: {} }] }
}
afterEach(() => { cleanup(); localStorage.clear(); resetExecution(); vi.restoreAllMocks() })
describe('workflow editor behavior', () => {
  it('searches across node categories, adds a node, and preserves search when the library is collapsed', () => {
    const flow = workflowStore.save(draft()); render(<WorkflowCanvas flow={flow} />)
    fireEvent.change(screen.getByLabelText('搜索节点'), { target: { value: '图片生成' } })
    expect(screen.queryByRole('button', { name: '添加上传图片' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '收起节点库' }))
    expect(screen.queryByLabelText('搜索节点')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '展开节点库' }))
    expect(screen.getByLabelText('搜索节点')).toHaveValue('图片生成')
    fireEvent.click(screen.getByRole('button', { name: '添加图片生成' }))
    expect(workflowStore.get(flow.id)?.nodes.at(-1)?.kind).toBe('image.generate')
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes).toHaveLength(2)
    fireEvent.change(screen.getByLabelText('搜索节点'), { target: { value: 'does-not-exist' } })
    expect(screen.getByText('没有匹配的节点，试试其他关键词。')).toBeVisible()
  })
  it('retains measured dimensions and actual ReactFlow handle bounds through edits without saving UI state', () => {
    const flow = workflowStore.save(draft()); render(<WorkflowCanvas flow={flow} />)
    const save = vi.spyOn(workflowStore, 'save')
    const lookup = new Map(), parents = new Map()
    adoptUserNodes(canvasProps.nodes!, lookup, parents)
    const dimensions = { width: 200, height: 140 }
    const handles = { source: [{ id: 'text', x: 200, y: 50, width: 10, height: 10, position: 'right' }], target: [] }
    lookup.get('prompt').measured = dimensions
    lookup.get('prompt').internals.handleBounds = handles
    act(() => canvasProps.onNodesChange?.([{ type: 'dimensions', id: 'prompt', dimensions }]))
    expect(save).not.toHaveBeenCalled()
    expect(screen.getByRole('button', { name: '撤销' })).toBeDisabled()
    fireEvent.click(screen.getByText('输入提示'))
    fireEvent.change(screen.getByLabelText('提示词'), { target: { value: '更长的提示词' } })
    adoptUserNodes(canvasProps.nodes!, lookup, parents)
    expect(lookup.get('prompt').measured).toEqual(dimensions)
    expect(lookup.get('prompt').internals.handleBounds).toEqual(handles)
    expect(workflowStore.get(flow.id)?.nodes[0]).not.toHaveProperty('measured')
    expect(workflowStore.get(flow.id)?.nodes[0]).not.toHaveProperty('selected')
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(screen.getByRole('button', { name: '撤销' })).toBeDisabled()
    expect(canvasProps.nodes?.[0].measured).toEqual(dimensions)
  })
  it.each([
    { label: 'different nodes', target: 'output', x: 305, time: 1001, release: false },
    { label: 'a pause on the same node', target: 'prompt', x: 10, time: 5000, release: false },
    { label: 'separate key presses', target: 'prompt', x: 10, time: 1001, release: true },
  ])('separates keyboard moves after $label instead of keeping an unlimited drag group', ({ target, x, time, release }) => {
    const flow = workflowStore.save(draft()); render(<WorkflowCanvas flow={flow} />)
    const now = vi.spyOn(Date, 'now').mockReturnValue(1000)
    act(() => canvasProps.onNodesChange?.([{ type: 'position', id: 'prompt', position: { x: 5, y: 0 }, dragging: false }]))
    if (release) fireEvent.keyUp(screen.getByLabelText('工作流画布'), { key: 'ArrowRight' })
    now.mockReturnValue(time)
    act(() => canvasProps.onNodesChange?.([{ type: 'position', id: target, position: { x, y: 0 }, dragging: false }]))
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes.map(n => n.position.x)).toEqual([5, 300])
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes.map(n => n.position.x)).toEqual([0, 300])
  })
  it('persists editing, coalesces typing, copies independently, reopens current snapshot and preserves input shortcuts', async () => {
    const flow = workflowStore.save(draft()), view = render(<WorkflowCanvas flow={flow} />)
    fireEvent.click(screen.getByText('输入提示'))
    const input = screen.getByLabelText('提示词')
    fireEvent.change(input, { target: { value: '新' } }); fireEvent.change(input, { target: { value: '新提示' } })
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 350)) })
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '新提示' })
    fireEvent.keyDown(input, { key: 'z', ctrlKey: true }); fireEvent.keyDown(input, { key: 'Delete' })
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '新提示' })
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '原文' })
    fireEvent.click(screen.getByRole('button', { name: '重做' })); fireEvent.click(screen.getByRole('button', { name: '复制节点' }))
    const copied = workflowStore.get(flow.id)!.nodes[2]
    fireEvent.click(screen.getByText(copied.title)); fireEvent.change(screen.getByLabelText('提示词'), { target: { value: '副本内容' } })
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 350)) })
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '新提示' })
    const saved = workflowStore.get(flow.id)!
    view.unmount(); render(<WorkflowCanvas flow={saved} />)
    fireEvent.click(screen.getByText(copied.title))
    expect((screen.getByLabelText('提示词') as HTMLTextAreaElement).value).toBe('副本内容')
  })
  it('treats a drag as one undo and removes incident edges in the same deletion step', () => {
    const flow = workflowStore.save(draft()); render(<WorkflowCanvas flow={flow} />)
    fireEvent.click(screen.getByText('drag')); fireEvent.click(screen.getByText('drop'))
    expect(workflowStore.get(flow.id)?.nodes[0].position).toEqual({ x: 150, y: 250 })
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes[0].position).toEqual({ x: 0, y: 0 })
    fireEvent.click(screen.getByText('connect')); fireEvent.click(screen.getByText('connect'))
    expect(screen.getByRole('status').textContent).toContain('已存在')
    expect(workflowStore.get(flow.id)?.edges).toHaveLength(1)
    fireEvent.click(screen.getByText('输入提示')); fireEvent.click(screen.getByRole('button', { name: '删除所选' }))
    expect(workflowStore.get(flow.id)?.edges).toHaveLength(0)
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.edges).toHaveLength(1)
    expect(workflowStore.get(flow.id)?.nodes).toHaveLength(2)
  })
  it('shows save failure, keeps unsaved snapshot editable and retries it without claiming success', async () => {
    const flow = workflowStore.save(draft()); render(<WorkflowCanvas flow={flow} />)
    const write = vi.spyOn(Object.getPrototypeOf(localStorage) as Storage, 'setItem').mockImplementation(() => { throw new Error('quota') })
    fireEvent.click(screen.getByText('输入提示')); fireEvent.change(screen.getByLabelText('节点名称'), { target: { value: '未丢失' } })
    expect(screen.getByText('正在编辑…')).toBeVisible()
    await act(async () => { await new Promise(resolve => setTimeout(resolve, 350)) })
    expect(screen.getByRole('alert').textContent).toContain('保存失败')
    expect(screen.queryByText('已保存到本机')).toBeNull()
    write.mockRestore(); fireEvent.click(screen.getByText('重试保存'))
    expect(workflowStore.get(flow.id)?.nodes[0].title).toBe('未丢失')
    await act(async () => { fireEvent.click(screen.getByText('检查配置')) })
    expect(screen.getByRole('status').textContent).toContain('补充文本内容')
  })
})

it('lets an open dropdown consume Escape before closing the node inspector', () => {
  render(<WorkflowCanvas flow={workflowStore.save(draft())} />)
  fireEvent.click(screen.getByText('输入提示'))
  const listbox = document.createElement('div'); listbox.setAttribute('role', 'listbox'); document.body.appendChild(listbox)
  fireEvent.keyDown(screen.getByLabelText('提示词'), { key: 'Escape' })
  expect(screen.getByLabelText('节点名称')).toBeVisible()
  listbox.remove()
  fireEvent.keyDown(screen.getByLabelText('提示词'), { key: 'Escape' })
  expect(screen.queryByLabelText('节点名称')).toBeNull()
})

it('starts from the live flow snapshot', () => {
  const flow = workflowStore.save(draft())
  render(<WorkflowCanvas flow={flow} />)
  fireEvent.click(screen.getByRole('button', { name: '运行' }))
  const getter = executionState.start.mock.calls[0][0] as () => typeof flow
  expect(getter().name).toBe('测试编排')
  fireEvent.change(screen.getByLabelText('工作流'), { target: { value: '改名后' } })
  expect(getter().name).toBe('改名后')
  expect(getter()).not.toBe(flow)
})

it('shows editing while a node is dragged and saved after the drop', () => {
  render(<WorkflowCanvas flow={workflowStore.save(draft())} />)
  expect(screen.getByText('已保存到本机')).toBeVisible()
  fireEvent.click(screen.getByText('drag'))
  expect(screen.getByText('正在编辑…')).toBeVisible()
  fireEvent.click(screen.getByText('drop'))
  expect(screen.getByText('已保存到本机')).toBeVisible()
})

it('recenters only when the focused node changes and keeps canvas inputs stable', () => {
  const fitView = vi.fn()
  vi.spyOn(window, 'requestAnimationFrame').mockImplementation(callback => { callback(0); return 1 })
  vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {})
  render(<WorkflowCanvas flow={workflowStore.save(draft())} />)
  act(() => { canvasProps.onInit?.({ fitView } as never) })
  fireEvent.click(screen.getByText('输入提示'))
  expect(fitView).toHaveBeenCalledTimes(1)
  expect(fitView).toHaveBeenCalledWith({ nodes: [{ id: 'prompt' }], padding: 0.25, maxZoom: 1 })
  fitView.mockClear()
  const nodes = canvasProps.nodes
  const edges = canvasProps.edges
  const valid = canvasProps.isValidConnection
  const end = canvasProps.onConnectEnd
  const options = canvasProps.proOptions
  fireEvent.click(screen.getByRole('button', { name: '收起节点库' }))
  expect(fitView).not.toHaveBeenCalled()
  expect(canvasProps.nodes).toBe(nodes)
  expect(canvasProps.edges).toBe(edges)
  expect(canvasProps.isValidConnection).toBe(valid)
  expect(canvasProps.onConnectEnd).toBe(end)
  expect(canvasProps.proOptions).toBe(options)
  fireEvent.click(screen.getByRole('button', { name: '运行记录' }))
  expect(fitView).not.toHaveBeenCalled()
})

it('colors canvas nodes from the active run while a history run stays selected', () => {
  const flow = workflowStore.save(draft())
  const active = recordedRun(flow, 'running', 'running', null)
  const history = { ...recordedRun(flow, 'failed', 'failed', null), id: 'old' }
  executionState.active = active
  executionState.selected = history
  executionState.runs = [active, history]
  executionState.busy = true
  render(<WorkflowCanvas flow={flow} />)
  expect(canvasProps.nodes?.find(node => node.id === 'prompt')?.data.status).toBe('running')
})

it('colors canvas nodes from the selected run when nothing is active', () => {
  const flow = workflowStore.save(draft())
  const history = recordedRun(flow, 'failed', 'cancelled', null)
  executionState.selected = history
  executionState.runs = [history]
  render(<WorkflowCanvas flow={flow} />)
  expect(canvasProps.nodes?.find(node => node.id === 'prompt')?.data.status).toBe('cancelled')
})

it('offers a result lookup when the failed node already has a media task', () => {
  const flow = workflowStore.save(draft())
  const failed = recordedRun(flow, 'failed', 'failed', 'media-1')
  executionState.runs = [failed]
  executionState.selected = failed
  render(<WorkflowCanvas flow={flow} />)
  fireEvent.click(screen.getByRole('button', { name: '运行记录' }))
  expect(screen.getByRole('button', { name: '查询已有结果' })).toBeVisible()
  expect(screen.getByText('将查询已有结果，不会重新生成。')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: '查询已有结果' }))
  expect(executionState.resume).toHaveBeenCalledWith(failed.id)
})

it('keeps continue wording when the failed node has no media task', () => {
  const flow = workflowStore.save(draft())
  const failed = recordedRun(flow, 'failed', 'failed', null)
  executionState.runs = [failed]
  executionState.selected = failed
  render(<WorkflowCanvas flow={flow} />)
  fireEvent.click(screen.getByRole('button', { name: '运行记录' }))
  expect(screen.getByRole('button', { name: '继续此运行' })).toBeVisible()
  expect(screen.queryByText('将查询已有结果，不会重新生成。')).toBeNull()
})
