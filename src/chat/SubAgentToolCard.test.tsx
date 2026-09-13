import { act, fireEvent, render, screen } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { ToolCallBlock } from './ToolCallBlock'
import { updateSubAgent } from './useSubAgents'
import { onDockSubAgentRequest } from './dock/dockPreview'

vi.mock('../api/tauri', () => ({ api: { chatSubagentControl: vi.fn(async () => ({ agents: [{ id: 'child', name: 'Research', sequence: 1, profile: { model: 'test', agentType: 'research' }, runs: [{ id: 'run', status: 'running', prompt: 'Inspect' }], history: [], tools: [], messages: [] }] })) } }))

it('tracks the launched execution and opens that child directly', async () => {
  const open = vi.fn()
  const unsubscribe = onDockSubAgentRequest(open)
  render(<ToolCallBlock toolCall={{ id: 'call', source: 'native', name: 'agent', status: 'success', arguments: { prompt: 'Inspect' }, structured_content: { type: 'subagent_started', conversation_id: 'conv', id: 'child', execution_id: 'run', name: 'Research' } }} />)
  await screen.findByText('运行中')
  expect(screen.queryByText('任务说明')).toBeNull()
  expect(screen.queryByText('research · test')).toBeNull()
  act(() => updateSubAgent('conv', { id: 'child', name: 'Research', sequence: 2, profile: { model: 'test', agentType: 'research' }, runs: [{ id: 'run', status: 'completed', prompt: 'Inspect' }, { id: 'next', status: 'running', prompt: 'Next' }], history: [], tools: [], messages: [] }))
  expect(screen.getByText('已完成')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: /Research/ }))
  expect(open).toHaveBeenCalledWith({ conversationId: 'conv', agentId: 'child' })
  unsubscribe()
})

it('summarizes legacy result receipts with the child status', () => {
  render(<ToolCallBlock toolCall={{ id: 'wait', source: 'native', name: 'agent_control', status: 'success', arguments: { operation: 'wait' }, result_preview: JSON.stringify({ reason: 'result_ready', waited_ms: 1250, agents: [{ id: 'child', name: 'Research', status: 'completed' }] }) }} />)
  expect(screen.getByText('子代理结果 · Research')).toBeVisible()
  expect(screen.getByText('已完成')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: /子代理结果/ }))
  expect(screen.getByText('Research')).toBeVisible()
  expect(screen.queryByText(/"waited_ms"/)).toBeNull()
})

it('keeps a stopping acknowledgement distinct from a stopped child', () => {
  render(<ToolCallBlock toolCall={{ id: 'stop', source: 'native', name: 'agent_control', status: 'success', arguments: { operation: 'stop' }, structured_content: { type: 'subagent_control', conversation_id: 'conv', id: 'child', name: 'Research', status: 'stopping' } }} />)
  expect(screen.getByText('正在停止')).toBeVisible()
  expect(screen.queryByText('已中断')).toBeNull()
})


it('collapses message contents and opens the addressed child from details', () => {
  const open = vi.fn()
  const unsubscribe = onDockSubAgentRequest(open)
  render(<ToolCallBlock toolCall={{ id: 'message', source: 'native', name: 'agent_control', status: 'success', arguments: { operation: 'message', message: '请补充取消流程' }, structured_content: { type: 'subagent_control', conversation_id: 'conv', id: 'child', name: '工具权限', status: 'running' } }} />)
  const summary = screen.getByRole('button', { name: /已向「工具权限」发送消息/ })
  expect(summary).toHaveAttribute('aria-expanded', 'false')
  expect(screen.queryByText('请补充取消流程')).toBeNull()
  fireEvent.click(summary)
  expect(screen.getByText('请补充取消流程')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: /工具权限.*运行中/ }))
  expect(open).toHaveBeenCalledWith({ conversationId: 'conv', agentId: 'child' })
  fireEvent.click(summary)
  expect(screen.queryByText('请补充取消流程')).toBeNull()
  unsubscribe()
})

it('does not claim a failed message was sent and keeps the error inspectable', () => {
  render(<ToolCallBlock toolCall={{ id: 'message-error', source: 'native', name: 'agent_control', status: 'error', arguments: { operation: 'message', id: 'child' }, error: '连接中断' }} />)
  expect(screen.queryByText(/已向/)).toBeNull()
  expect(screen.getByText('操作未完成')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: /发送消息/ }))
  expect(screen.getByRole('alert')).toHaveTextContent('连接中断')
})

it('keeps saved output collapsed and labels a continued receipt by its execution status', () => {
  render(<ToolCallBlock toolCall={{ id: 'continue', source: 'native', name: 'agent_control', status: 'success', arguments: { operation: 'continue' }, structured_content: { type: 'subagent_control', conversation_id: 'conv', id: 'child', name: '工具权限', status: 'running', result: '已保存的调查内容' } }} />)
  expect(screen.getByText('运行中')).toBeVisible()
  expect(screen.queryByText('已保存的调查内容')).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: /继续子代理/ }))
  expect(screen.getByText('已保存的调查内容')).toBeVisible()
})


it.each(['timeout', 'user_input'])('removes a finished %s wait without changing child state', (reason) => {
  const call = { id: 'wait', source: 'native', name: 'agent_control', arguments: { operation: 'wait' } }
  const { rerender, container } = render(<ToolCallBlock toolCall={{ ...call, status: 'running' }} />)
  expect(screen.getByRole('status')).toHaveTextContent('正在等待结果')
  expect(screen.queryByRole('button')).toBeNull()
  rerender(<ToolCallBlock toolCall={{ ...call, status: 'success', structured_content: { type: 'subagent_control', reason, agents: [{ id: 'child', name: '工具权限', status: 'running' }] } }} />)
  expect(container).toBeEmptyDOMElement()
})

it('also hides historical waits stored as JSON', () => {
  const { container } = render(<ToolCallBlock toolCall={{ id: 'old-wait', source: 'native', name: 'agent_control', status: 'success', arguments: JSON.stringify({ operation: 'wait' }), result_preview: JSON.stringify({ reason: 'timeout', waited_ms: 60000 }) }} />)
  expect(container).toBeEmptyDOMElement()
})

it('retains actual wait errors even when the receipt mentions timeout', () => {
  render(<ToolCallBlock toolCall={{ id: 'wait-error', source: 'native', name: 'agent_control', status: 'error', arguments: { operation: 'wait' }, error: '无法读取子代理状态', structured_content: { type: 'subagent_control', reason: 'timeout' } }} />)
  expect(screen.getByText('操作未完成')).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: /等待子代理/ }))
  expect(screen.getByRole('alert')).toHaveTextContent('无法读取子代理状态')
})
