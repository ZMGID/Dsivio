import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type Settings } from '../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../api/settingsCache'
import { makeProvider, makeSettings } from '../../settings/tabs/testFixtures'
import { WorkbenchMediaModelSelect } from './WorkbenchMediaModelSelect'

vi.mock('../../api/settingsCache',()=>({getSettingsCached:vi.fn(),subscribeSettings:vi.fn()}))
let listener: (settings:Settings)=>void
beforeEach(()=>{localStorage.clear();vi.mocked(subscribeSettings).mockImplementation(fn=>{listener=fn;return ()=>{}})})

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
  it('opens the configured ComfyUI form inside Workbench when its pool entry is selected', async () => {
    vi.spyOn(api, 'listComfyTasks').mockResolvedValue([])
    const workflow = { id: 'wf', name: '商品换背景', kind: 'image' as const, graph: { '1': { class_type: 'CLIPTextEncode', inputs: { text: '白色背景' } } }, inputs: [{ nodeId: '1', input: 'text', label: '提示词', kind: 'text' as const }], outputNodes: ['1'] }
    const settings = makeSettings({ providers: [makeProvider({ enabledModels: ['wf'], request: { comfy: { workflows: [workflow] } } })], workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'wf' }], videoModels: [] } })
    vi.mocked(getSettingsCached).mockResolvedValue(settings)
    render(<WorkbenchMediaModelSelect kind="imageModels"><div>原图片工具</div></WorkbenchMediaModelSelect>)
    await userEvent.click(await screen.findByRole('button', { name: '图片模型' }))
    await userEvent.click(screen.getByRole('option', { name: 'OpenAI / 商品换背景' }))
    expect(await screen.findByLabelText('提示词')).toHaveValue('白色背景')
    expect(screen.getByRole('button', { name: '开始生成' })).toBeInTheDocument()
    expect(screen.queryByText('原图片工具')).toBeNull()
  })

})
