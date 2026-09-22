import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { makeProvider } from '../../settings/tabs/testFixtures'
import type { ComfyWorkflow } from '../../generated/comfyui'
import type { MediaTask } from '../../generated/mediaGeneration'
import { MediaGenerationRunner } from './MediaGenerationRunner'

const workflow: ComfyWorkflow = { id: 'wf', name: '商品图', kind: 'image', graph: { '1': { class_type: 'CLIPTextEncode', inputs: { text: 'original', seed: 10 } } }, inputs: [{ nodeId: '1', input: 'text', kind: 'text', label: '提示词' }, { nodeId: '1', input: 'seed', kind: 'number', label: '种子' }], outputNodes: ['1'] }
const provider = makeProvider({ request: { comfy: { workflows: [workflow] } } })
const task: MediaTask = { id: 'task', providerId: provider.id, model: 'wf', kind: 'image', remoteId: 'remote-task', status: 'succeeded', error: null, outputs: [], createdAt: new Date().toISOString(), canResume: false, origin: null, prompt: '' }
beforeEach(() => {
  vi.restoreAllMocks()
  vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
  vi.spyOn(api, 'getMediaTask').mockResolvedValue({ ...task, status: 'succeeded' })
  vi.spyOn(api, 'startMediaGeneration').mockResolvedValue(task)
})
describe('Workbench ComfyUI execution', () => {
  it('submits typed mapped values once, then resumes the persisted task on reopening', async () => {
    const view = render(<MediaGenerationRunner provider={provider} model="wf" kind="image" />)
    await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
    await userEvent.clear(screen.getByLabelText('提示词')); await userEvent.type(screen.getByLabelText('提示词'), 'new product')
    await userEvent.clear(screen.getByLabelText('种子')); await userEvent.type(screen.getByLabelText('种子'), '42')
    vi.mocked(api.listMediaTasks).mockResolvedValue([task])
    await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
    expect(api.startMediaGeneration).toHaveBeenCalledWith({providerId:provider.id,model:'wf',kind:'image',prompt:'',images:[],options:{ '1:text': 'new product', '1:seed': 42 },origin:null})
    expect(await screen.findByText('完成')).toBeInTheDocument()
    view.unmount()
    render(<MediaGenerationRunner provider={provider} model="wf" kind="image" />)
    expect(await screen.findByText('完成')).toBeInTheDocument()
    expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
  })
  it('prevents duplicate submits and shows uncertain receipt without retrying it', async () => {
    let finish!: (value: MediaTask) => void
    vi.mocked(api.startMediaGeneration).mockImplementation(() => new Promise(resolve => { finish = resolve }))
    render(<MediaGenerationRunner provider={provider} model="wf" kind="image" />)
    const button = screen.getByRole('button', { name: '开始生成' })
    await waitFor(() => expect(button).toBeEnabled())
    await userEvent.dblClick(button)
    expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
    expect(button).toBeDisabled()
    const uncertain = { ...task, remoteId: null, status: 'failed' as const, error: '请核查 ComfyUI 队列' }
    vi.mocked(api.listMediaTasks).mockResolvedValue([uncertain])
    finish(uncertain)
    expect(await screen.findByText('失败')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('请核查 ComfyUI 队列')
    expect(api.getMediaTask).not.toHaveBeenCalled()
    expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
  })
})

it('uses the same generation command for cloud video and restores query failures without submitting again', async () => {
  const cloud = makeProvider({ enabledModels: ['MiniMax-H3'] })
  const failed = { ...task, model: 'MiniMax-H3', kind: 'video' as const, status: 'failed' as const, error: '下载中断', canResume: true }
  vi.mocked(api.listMediaTasks).mockResolvedValue([failed])
  render(<MediaGenerationRunner provider={cloud} model="MiniMax-H3" kind="video" />)
  await screen.findByText('下载中断')
  await userEvent.click(screen.getByRole('button', { name: '恢复查询／下载' }))
  expect(api.getMediaTask).toHaveBeenCalledWith('task', true)
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
  await userEvent.type(screen.getByLabelText('提示词'), '商品展示')
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  expect(api.startMediaGeneration).toHaveBeenCalledWith({ providerId: cloud.id, model: 'MiniMax-H3', kind: 'video', prompt: '商品展示', images: [], options: {}, origin: null })
  expect(screen.queryByText('剧本确认')).toBeNull()
})

it('generates cloud images through the same entry without a planning stage', async () => {
  render(<MediaGenerationRunner provider={makeProvider()} model="gpt-image-1" kind="image" />)
  await userEvent.type(screen.getByLabelText('提示词'), '白底商品图')
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  expect(api.startMediaGeneration).toHaveBeenCalledWith(expect.objectContaining({ kind: 'image', model: 'gpt-image-1', prompt: '白底商品图', images: [] }))
})
