/** @vitest-environment jsdom */
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ComponentProps } from 'react'
import type { ReactFlow } from '@xyflow/react'
import { adoptUserNodes } from '@xyflow/system'
import { WorkflowCanvas } from './WorkflowCanvas'
import { blankWorkflow } from './workflowModel'
import { workflowStore } from './workflowStore'

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
const draft = () => ({ ...blankWorkflow('测试编排'), nodes: [
  { id: 'prompt', kind: 'prompt.input' as const, title: '输入提示', position: { x: 0, y: 0 }, config: { type: 'prompt' as const, text: '原文' } },
  { id: 'output', kind: 'text.preview' as const, title: '文本输出', position: { x: 300, y: 0 }, config: { type: 'output' as const } },
] })
afterEach(() => { cleanup(); localStorage.clear(); vi.restoreAllMocks() })
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
  it('persists editing, coalesces typing, copies independently, reopens current snapshot and preserves input shortcuts', () => {
    const flow = workflowStore.save(draft()), view = render(<WorkflowCanvas flow={flow} />)
    fireEvent.click(screen.getByText('输入提示'))
    const input = screen.getByLabelText('提示词')
    fireEvent.change(input, { target: { value: '新' } }); fireEvent.change(input, { target: { value: '新提示' } })
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '新提示' })
    fireEvent.keyDown(input, { key: 'z', ctrlKey: true }); fireEvent.keyDown(input, { key: 'Delete' })
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '新提示' })
    fireEvent.click(screen.getByRole('button', { name: '撤销' }))
    expect(workflowStore.get(flow.id)?.nodes[0].config).toMatchObject({ text: '原文' })
    fireEvent.click(screen.getByRole('button', { name: '重做' })); fireEvent.click(screen.getByRole('button', { name: '复制节点' }))
    const copied = workflowStore.get(flow.id)!.nodes[2]
    fireEvent.click(screen.getByText(copied.title)); fireEvent.change(screen.getByLabelText('提示词'), { target: { value: '副本内容' } })
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
    const write = vi.spyOn(localStorage, 'setItem').mockImplementation(() => { throw new Error('quota') })
    fireEvent.click(screen.getByText('输入提示')); fireEvent.change(screen.getByLabelText('节点名称'), { target: { value: '未丢失' } })
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
