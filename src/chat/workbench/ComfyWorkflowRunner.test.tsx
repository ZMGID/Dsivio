import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { makeProvider } from '../../settings/tabs/testFixtures'
import type { ComfyTask, ComfyWorkflow } from '../../generated/comfyui'
import { ComfyWorkflowRunner } from './ComfyWorkflowRunner'

const workflow: ComfyWorkflow = { id: 'wf', name: '商品图', kind: 'image', graph: { '1': { class_type: 'CLIPTextEncode', inputs: { text: 'original', seed: 10 } } }, inputs: [{ nodeId: '1', input: 'text', kind: 'text', label: '提示词' }, { nodeId: '1', input: 'seed', kind: 'number', label: '种子' }], outputNodes: ['1'] }
const provider = makeProvider({ request: { comfy: { workflows: [workflow] } } })
const task: ComfyTask = { id: 'task', providerId: provider.id, workflowId: 'wf', workflowName: '商品图', kind: 'image', baseUrl: 'http://localhost:8188', outputNodes: ['1'], promptId: 'remote-task', status: 'queued', error: null, outputs: [], createdAt: new Date().toISOString() }
beforeEach(() => {
  vi.restoreAllMocks()
  vi.spyOn(api, 'listComfyTasks').mockResolvedValue([])
  vi.spyOn(api, 'refreshComfyTask').mockResolvedValue({ ...task, status: 'succeeded' })
  vi.spyOn(api, 'submitComfyWorkflow').mockResolvedValue(task)
})
describe('Workbench ComfyUI execution', () => {
  it('submits typed mapped values once, then resumes the persisted task on reopening', async () => {
    const view = render(<ComfyWorkflowRunner provider={provider} workflow={workflow} />)
    await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
    await userEvent.clear(screen.getByLabelText('提示词')); await userEvent.type(screen.getByLabelText('提示词'), 'new product')
    await userEvent.clear(screen.getByLabelText('种子')); await userEvent.type(screen.getByLabelText('种子'), '42')
    vi.mocked(api.listComfyTasks).mockResolvedValue([task])
    await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
    expect(api.submitComfyWorkflow).toHaveBeenCalledWith(provider.id, 'wf', { '1:text': 'new product', '1:seed': 42 })
    expect(await screen.findByText('已完成')).toBeInTheDocument()
    view.unmount()
    render(<ComfyWorkflowRunner provider={provider} workflow={workflow} />)
    expect(await screen.findByText('已完成')).toBeInTheDocument()
    expect(api.submitComfyWorkflow).toHaveBeenCalledTimes(1)
    expect(api.refreshComfyTask).toHaveBeenCalledWith('task')
  })
  it('prevents duplicate submits and shows uncertain receipt without retrying it', async () => {
    let finish!: (value: ComfyTask) => void
    vi.mocked(api.submitComfyWorkflow).mockImplementation(() => new Promise(resolve => { finish = resolve }))
    render(<ComfyWorkflowRunner provider={provider} workflow={workflow} />)
    const button = screen.getByRole('button', { name: '开始生成' })
    await waitFor(() => expect(button).toBeEnabled())
    await userEvent.dblClick(button)
    expect(api.submitComfyWorkflow).toHaveBeenCalledTimes(1)
    expect(button).toBeDisabled()
    const uncertain = { ...task, promptId: null, status: 'uncertain' as const, error: '请核查 ComfyUI 队列' }
    vi.mocked(api.listComfyTasks).mockResolvedValue([uncertain])
    finish(uncertain)
    expect(await screen.findByText('提交待核查')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('请核查 ComfyUI 队列')
    expect(api.refreshComfyTask).not.toHaveBeenCalled()
    expect(api.submitComfyWorkflow).toHaveBeenCalledTimes(1)
  })
})
