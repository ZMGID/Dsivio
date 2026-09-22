import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Settings } from '../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../api/settingsCache'
import { makeProvider, makeSettings } from '../../settings/tabs/testFixtures'
import { PosterPage } from './image/PosterPage'
import { WorkbenchMediaModelSelect } from './WorkbenchMediaModelSelect'

vi.mock('../../api/settingsCache',()=>({getSettingsCached:vi.fn(),subscribeSettings:vi.fn()}))
let listener: (settings:Settings)=>void
beforeEach(()=>{vi.spyOn(api, 'listMediaTasks').mockResolvedValue([]);localStorage.clear();vi.mocked(subscribeSettings).mockImplementation(fn=>{listener=fn;return ()=>{}})})

describe('Workbench pool selection',()=>{
  it('offers only pool members, persists the explicit choice, and never auto-switches after removal',async()=>{
    const settings=makeSettings({providers:[makeProvider({enabledModels:['MiniMax-H3','grok-imagine-video','chat-only']})],workbenchMedia:{imageModels:[],videoModels:[{providerId:'p1',model:'MiniMax-H3'},{providerId:'p1',model:'grok-imagine-video'}]}})
    vi.mocked(getSettingsCached).mockResolvedValue(settings)
    const view=render(<WorkbenchMediaModelSelect kind="videoModels" />)
    const button=await screen.findByRole('button',{name:'视频模型'})
    await userEvent.click(button)
    expect(screen.queryByRole('option',{name:/chat-only/})).toBeNull()
    await userEvent.click(screen.getByRole('option',{name:'OpenAI / grok-imagine-video'}))
    expect(button).toHaveTextContent('grok-imagine-video')
    view.unmount()
    render(<WorkbenchMediaModelSelect kind="videoModels" />)
    expect(await screen.findByRole('button',{name:'视频模型'})).toHaveTextContent('grok-imagine-video')
    act(()=>listener({...settings,workbenchMedia:{imageModels:[],videoModels:[{providerId:'p1',model:'MiniMax-H3'}]}}))
    expect(screen.getByRole('button',{name:'视频模型'})).toHaveTextContent('请选择本次使用的模型')
    expect(screen.getByText('上次选择的模型已不可用，请重新选择。')).toBeInTheDocument()
  })
  it('keeps the existing page when opening the configured ComfyUI form inside Workbench when its pool entry is selected', async () => {
    vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
    const workflow = { id: 'wf', name: '商品换背景', kind: 'image' as const, graph: { '1': { class_type: 'CLIPTextEncode', inputs: { text: '白色背景' } } }, inputs: [{ nodeId: '1', input: 'text', label: '提示词', kind: 'text' as const }], outputNodes: ['1'] }
    const settings = makeSettings({ providers: [makeProvider({ enabledModels: ['wf'], request: { comfy: { workflows: [workflow] } } })], workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'wf' }], videoModels: [] } })
    vi.mocked(getSettingsCached).mockResolvedValue(settings)
    render(<WorkbenchMediaModelSelect kind="imageModels"><div>原图片工具</div></WorkbenchMediaModelSelect>)
    await userEvent.click(await screen.findByRole('button', { name: '图片模型' }))
    await userEvent.click(screen.getByRole('option', { name: 'OpenAI / 商品换背景' }))
    expect(await screen.findByLabelText('提示词')).toHaveValue('白色背景')
    expect(screen.getByRole('button', { name: '开始生成' })).toBeInTheDocument()
    expect(screen.getByText('原图片工具')).toBeVisible()
  })

})

it('keeps the real poster form editable with no model and preserves input across selection changes', async () => {
  vi.mocked(getSettingsCached).mockResolvedValue(makeSettings())
  render(<PosterPage />)
  const brief = screen.getByRole('textbox')
  await userEvent.type(brief, '夏季促销海报')
  expect(await screen.findByText('请先在「设置 → 媒体创作」中添加可用模型。')).toBeVisible()
  expect(brief).toBeVisible()
  expect(brief).toBeEnabled()
  const provider = makeProvider({ enabledModels: ['gpt-image-1'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } } } })
  act(() => listener(makeSettings({ providers: [provider], workbenchMedia: { imageModels: [{ providerId: provider.id, model: 'gpt-image-1' }], videoModels: [] } })))
  await userEvent.click(screen.getByRole('button', { name: '图片模型' }))
  await userEvent.click(screen.getByRole('option', { name: 'OpenAI / gpt-image-1' }))
  expect(screen.getByRole('textbox')).toBe(brief)
  expect(brief).toHaveValue('夏季促销海报')
  act(() => listener(makeSettings()))
  expect(brief).toBeVisible()
  expect(brief).toHaveValue('夏季促销海报')
})

it('keeps page settings available when loading the model pool fails', async () => {
  vi.mocked(getSettingsCached).mockRejectedValueOnce(new Error('offline'))
  render(<PosterPage />)
  await screen.findByRole('alert')
  expect(screen.getByRole('textbox')).toBeVisible()
  await userEvent.type(screen.getByRole('textbox'), '可继续填写')
  expect(screen.getByRole('textbox')).toHaveValue('可继续填写')
})

it('keeps a controlled workflow selection isolated from the saved page choice', async () => {
  const settings = makeSettings({ providers: [makeProvider({ enabledModels: ['gpt-image-1'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } } } })], workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'gpt-image-1' }], videoModels: [] } })
  vi.mocked(getSettingsCached).mockResolvedValue(settings)
  localStorage.setItem('dsivio.workbench.media-choice.imageModels', 'existing-page-choice')
  const change = vi.fn()
  render(<WorkbenchMediaModelSelect kind="imageModels" value="" onChange={change} render={control => control} />)
  await userEvent.click(await screen.findByRole('button', { name: '图片模型' }))
  await userEvent.click(screen.getByRole('option', { name: 'OpenAI / gpt-image-1' }))
  expect(change).toHaveBeenCalledWith('p1', 'gpt-image-1', settings.providers[0])
  expect(localStorage.getItem('dsivio.workbench.media-choice.imageModels')).toBe('existing-page-choice')
})
