import { act, fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { InputBar } from './InputBar'
import { derivePermissionModes } from './permissionModes'
import type { AgentRuntimeConfig } from './types'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onFocusChanged: () => Promise.resolve(() => {}) }),
}))
vi.mock('../api/tauri', () => ({ api: {}, isTauriRuntime: () => false }))
vi.mock('./api', () => ({
  chatApi: {
    getProjects: () => Promise.resolve([]),
    listExternalCliSlashCommands: () => Promise.resolve({ commands: [] }),
  },
}))

function renderComposer(modes: { options: ReturnType<typeof derivePermissionModes>['options']; current: string }, onModeChange = vi.fn()) {
  render(
    <InputBar
      onSend={() => {}}
      modeOptions={modes.options}
      modeValue={modes.current}
      onModeChange={onModeChange}
    />,
  )
  return onModeChange
}

function openModeMenu(pillLabel: string) {
  act(() => {
    fireEvent.click(screen.getByTitle('切换模式'))
  })
  expect(screen.getByTitle('切换模式')).toHaveTextContent(pillLabel)
}

describe('InputBar 底栏模式胶囊', () => {
  it('Goal occupies the composer status row and replaces its Todo indicator', () => {
    render(<InputBar onSend={() => {}} goalSlot={<div data-testid="goal">Goal status</div>}
      agentTodoState={{ items: [{ id: 'todo', content: 'pending work', status: 'pending' }] }} />)
    expect(screen.getByTestId('goal').closest('.chat-composer-status')).toBeInTheDocument()
    expect(screen.queryByText('Todo')).not.toBeInTheDocument()
  })

  it('内置模型会话显示 Act / Goal / Plan / Orchestrate 四档', () => {
    const runtime: AgentRuntimeConfig = { kind: 'builtin' }
    renderComposer(derivePermissionModes({
      target: 'composer',
      agentRuntime: runtime,
      agentPlanMode: 'act',
    }))
    openModeMenu('Act')
    const items = screen.getAllByRole('menuitemradio')
    expect(items.map((item) => item.textContent)).toEqual([
      'Act普通模式 · Normal',
      'Goal持续执行一个目标 · Persistent execution',
      'Plan生成计划文档 · Plan document',
      'Orchestrate主代理统筹并行协作 · Parallel collaboration',
    ])
    expect(items[0]).toHaveAttribute('aria-checked', 'true')
  })


  it('该 CLI 没有档位时胶囊整个不渲染', () => {
    const runtime: AgentRuntimeConfig = { kind: 'external', externalAgentId: 'opencode' }
    renderComposer(derivePermissionModes({
      target: 'composer',
      agentRuntime: runtime,
    }))
    expect(screen.queryByTitle('切换模式')).not.toBeInTheDocument()
  })




})
