import { useState } from 'react'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, type ModelProvider } from '../api/tauri'
import { makeProvider, makeSettings } from './tabs/testFixtures'
import { ComfyProviderPanel } from './ComfyProviderPanel'
import { MediaCreationTab } from './tabs/MediaCreationTab'
import { applyProviderDraftIntent } from './providerDraftIntents'
import { PROVIDER_PRESETS } from './providerPresets'
import { buildModelPairOptions } from './utils'

const graph = { '6': { class_type: 'CLIPTextEncode', inputs: { text: 'a product' } }, '9': { class_type: 'SaveImage', inputs: { filename_prefix: 'output' } } }
function Fixture() {
  const [settings, setSettings] = useState(applyProviderDraftIntent(makeSettings(), { type: 'add', id: 'comfy', preset: PROVIDER_PRESETS.find(p => p.comfy) }))
  const provider = settings.providers[0]
  return <><ComfyProviderPanel provider={provider} lang="zh" onUpdateProvider={(id, updates) => setSettings(s => applyProviderDraftIntent(s, { type: 'update', id, updates }))} /><MediaCreationTab settings={settings} lang="zh" onUpdatePool={(kind, models) => setSettings(s => ({ ...s, workbenchMedia: { ...s.workbenchMedia, [kind]: models } }))} /><output data-testid="settings">{JSON.stringify(settings)}</output></>
}
async function upload(data: unknown) {
  const file = new File([JSON.stringify(data)], '商品精修.json', { type: 'application/json' })
  Object.defineProperty(file, 'text', { value: async () => JSON.stringify(data) })
  await userEvent.upload(screen.getByLabelText('导入 API 工作流 JSON'), file)
}
beforeEach(() => { vi.restoreAllMocks(); vi.spyOn(api, 'validateComfyWorkflow').mockResolvedValue(); vi.spyOn(api, 'testComfyConnection').mockResolvedValue({ devices: ['GPU'], missingNodes: [] }) })
describe('ComfyUI provider configuration', () => {
  it('imports, maps inputs, and adds named workflows to the independent media pool', async () => {
    render(<Fixture />)
    await upload(graph)
    await screen.findByDisplayValue('商品精修')
    await userEvent.click(screen.getByRole('button', { name: '添加参数' }))
    const label = screen.getByLabelText('参数名称 1')
    await userEvent.clear(label); await userEvent.type(label, '提示词')
    await userEvent.click(screen.getByRole('button', { name: '完成配置' }))
    const poolSwitch = await screen.findByRole('switch', { name: 'ComfyUI 本地 / 商品精修' })
    await userEvent.click(poolSwitch)
    const saved = JSON.parse(screen.getByTestId('settings').textContent!)
    expect(saved.providers[0].baseUrl).toBe('http://127.0.0.1:8188')
    expect(saved.providers[0].request.comfy.workflows[0].inputs[0]).toMatchObject({ nodeId: '6', input: 'text', kind: 'text', label: '提示词' })
    expect(saved.workbenchMedia.imageModels).toHaveLength(1)
    expect(saved.workbenchMedia.videoModels).toHaveLength(0)
    expect(saved.defaultModels.imageGeneration.providerId).toBe('')
  })
  it('rejects canvas JSON and displays connection failures without losing imported data', async () => {
    render(<Fixture />)
    await upload({ nodes: [], links: [] })
    expect(await screen.findByRole('alert')).toHaveTextContent('API 格式')
    await upload(graph)
    await screen.findByDisplayValue('商品精修')
    vi.mocked(api.testComfyConnection).mockRejectedValueOnce('连接失败')
    await userEvent.click(screen.getByRole('button', { name: '测试连接与节点' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('连接失败')
    expect(screen.getByDisplayValue('商品精修')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: '测试连接与节点' })).toBeEnabled())
  })
  it('never offers ComfyUI workflows as conversation model assignments', () => {
    const provider = makeProvider({ request: { comfy: { workflows: [{ id: 'wf', name: 'My workflow', kind: 'image', graph, inputs: [], outputNodes: ['9'] }] } }, enabledModels: ['wf'] }) as ModelProvider
    expect(buildModelPairOptions([provider])).toEqual([])
  })
})
