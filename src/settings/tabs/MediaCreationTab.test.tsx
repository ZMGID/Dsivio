import { useState } from 'react'
import { act, fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import type { Settings, VoiceReference } from '../../api/tauri'
import type * as Tauri from '../../api/tauri'
import { MediaCreationTab } from './MediaCreationTab'
import { makeProvider, makeSettings } from './testFixtures'

vi.mock('../../api/tauri', async importActual => {
  const actual = await importActual<typeof Tauri>()
  return { ...actual, api: { ...actual.api, getLocalAsrStatus: vi.fn(), listMediaVoices: vi.fn(), installLocalAsr: vi.fn(), cancelLocalAsrInstall: vi.fn(), stopLocalAsr: vi.fn(), deleteMediaVoice: vi.fn(), checkMediaSpeechConnection: vi.fn() } }
})
const asrStatus = {
  operationId: null, state: 'notInstalled', installationId: null, serviceVersion: '0.2.0', model: 'small', languages: ['en', 'zh'],
  progress: null, error: null, runtime: { state: 'stopped', pid: null, activeTaskId: null },
}
const voiceReference: VoiceReference = { id: 'p1/voice_fixture', providerId: 'p1', voiceId: 'voice_fixture', protocol: 'openai_tts',
  consentSha256: '', createdAt: '2026-10-01T00:00:00Z', expiresAt: null, used: true, lastUsedAt: null, preview: null }
beforeEach(() => {
  vi.mocked(api.getLocalAsrStatus).mockResolvedValue(asrStatus)
  vi.mocked(api.listMediaVoices).mockResolvedValue([])
})

const imageModels = ['image-a', 'image-b', 'image-c']
function Fixture({ initial = makeSettings({ providers: [makeProvider({
  enabledModels: [...imageModels, 'MiniMax-H3', 'grok-imagine-video', 'chat-only'],
  modelOverrides: Object.fromEntries(imageModels.map(model => [model, { capabilities: { imageGeneration: true } }])),
})] }) }: { initial?: Settings }) {
  const [settings, setSettings] = useState(initial)
  return <><MediaCreationTab settings={settings} lang="zh"
    onUpdatePool={(kind, models) => setSettings(s => ({ ...s, workbenchMedia: { ...s.workbenchMedia, [kind]: models } }))}
    onUpdateLocalAsr={localAsr => setSettings(s => ({ ...s, workbenchMedia: { ...s.workbenchMedia, localAsr } }))} /><output data-testid="settings">{JSON.stringify(settings)}</output></>
}
const savedSettings = () => JSON.parse(screen.getByTestId('settings').textContent!) as Settings

describe('Workbench media pools', () => {
  it('shows models directly and selects multiple image and video models independently of chat', async () => {
    render(<Fixture />)
    const original = savedSettings().defaultModels
    const images = within(screen.getByRole('region', { name: '图片模型池' }))
    expect(images.queryByRole('switch', { name: /chat-only|MiniMax-H3/ })).toBeNull()
    for (const model of [...imageModels, 'MiniMax-H3', 'grok-imagine-video']) {
      await userEvent.click(screen.getByRole('switch', { name: `OpenAI / ${model}` }))
    }
    expect(savedSettings().workbenchMedia.imageModels).toHaveLength(3)
    expect(savedSettings().workbenchMedia.videoModels).toHaveLength(2)
    expect(savedSettings().defaultModels).toEqual(original)
    await userEvent.click(images.getByRole('switch', { name: 'OpenAI / image-b' }))
    expect(savedSettings().workbenchMedia.imageModels.map(m => m.model)).toEqual(['image-a', 'image-c'])
    expect(savedSettings().workbenchMedia.videoModels).toHaveLength(2)
  })

  it('shows complete similar model names and searches model or provider without clearing selections', async () => {
    const models = ['gemini-3.1-flash-image-preview', 'gemini-3-pro-image-preview']
    render(<Fixture initial={makeSettings({ providers: [makeProvider({ name: '图片供应商', enabledModels: models,
      modelOverrides: Object.fromEntries(models.map(model => [model, { capabilities: { imageGeneration: true } }])),
    })] })} />)
    // Both names are present without opening a menu, even when their prefixes match.
    for (const model of models) expect(screen.getByText(model)).toBeTruthy()
    const search = screen.getByRole('searchbox', { name: '搜索图片模型' })
    await userEvent.type(search, '3.1')
    expect(screen.queryByRole('switch', { name: `图片供应商 / ${models[1]}` })).toBeNull()
    await userEvent.click(screen.getByRole('switch', { name: `图片供应商 / ${models[0]}` }))
    await userEvent.clear(search)
    await userEvent.type(search, '图片供应商')
    expect(screen.getByRole('switch', { name: `图片供应商 / ${models[0]}` }).getAttribute('aria-checked')).toBe('true')
    await userEvent.clear(search)
    await userEvent.type(search, 'no-match')
    expect(screen.getByText('没有匹配的模型')).toBeTruthy()
    expect(savedSettings().workbenchMedia.imageModels).toHaveLength(1)
  })

  const order = () => savedSettings().workbenchMedia.imageModels.map(m => m.model)
  const pointer = (type: string, clientY: number) => new MouseEvent(type, { clientY, bubbles: true, cancelable: true })
  // jsdom rows have no height, so one row is the hook's 30px fallback plus its default 1px gap.
  const drag = (handle: HTMLElement, rows: number) => {
    fireEvent(handle, pointer('pointerdown', 0))
    fireEvent(document, pointer('pointermove', rows * 31))
    fireEvent(document, pointer('pointerup', rows * 31))
  }
  async function selectAll() {
    for (const model of imageModels) await userEvent.click(screen.getByRole('switch', { name: `OpenAI / ${model}` }))
  }

  it('lists selected models first in pool order, marks the first as the default, and dragging changes the priority', async () => {
    render(<Fixture />)
    await selectAll()
    const images = within(screen.getByRole('region', { name: '图片模型池' }))
    expect(order()).toEqual(['image-a', 'image-b', 'image-c'])
    expect(images.getAllByText('默认')).toHaveLength(1)
    expect(images.getByText('image-a').parentElement!.textContent).toContain('默认')
    // Drag the last row to the top: it becomes the default and the saved pool order follows.
    drag(images.getByRole('button', { name: /拖动调整优先级: OpenAI \/ image-c/ }), -2)
    expect(order()).toEqual(['image-c', 'image-a', 'image-b'])
    expect(images.getByText('image-c').parentElement!.textContent).toContain('默认')
    expect(images.getByText('image-a').parentElement!.textContent).not.toContain('默认')
    // Video pool and chat defaults are untouched by reordering images.
    expect(savedSettings().workbenchMedia.videoModels).toEqual([])
  })

  it('shows selected models above unselected ones, appends a newly selected model last, and keeps order across toggles', async () => {
    render(<Fixture />)
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-c' }))
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-a' }))
    expect(order()).toEqual(['image-c', 'image-a'])
    const names = within(screen.getByRole('region', { name: '图片模型池' })).getAllByRole('switch').map(item => item.getAttribute('aria-label'))
    expect(names).toEqual(['OpenAI / image-c', 'OpenAI / image-a', 'OpenAI / image-b'])
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-c' }))
    expect(order()).toEqual(['image-a'])
  })

  it('offers dragging only for two or more selected models and not while a search hides part of the pool', async () => {
    render(<Fixture />)
    const images = within(screen.getByRole('region', { name: '图片模型池' }))
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-a' }))
    expect(images.queryByRole('button', { name: /拖动调整优先级/ })).toBeNull()
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-b' }))
    expect(images.getAllByRole('button', { name: /拖动调整优先级/ })).toHaveLength(2)
    await userEvent.type(screen.getByRole('searchbox', { name: '搜索图片模型' }), 'image')
    expect(images.queryByRole('button', { name: /拖动调整优先级/ })).toBeNull()
  })

  it('keeps unavailable members visible and removable without deleting their provider', async () => {
    render(<Fixture initial={makeSettings({ providers: [makeProvider({ enabled: false, enabledModels: ['image-a'] })],
      workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'image-a' }], videoModels: [] },
    })} />)
    expect(screen.getByText(/当前不可用/)).toBeTruthy()
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-a' }))
    expect(savedSettings().workbenchMedia.imageModels).toEqual([])
    expect(savedSettings().providers).toHaveLength(1)
  })
})

describe('Media service controls', () => {
  it('preserves a failed installation and offers retry instead of presenting it as ready', async () => {
    vi.mocked(api.getLocalAsrStatus).mockResolvedValue({ ...asrStatus, state: 'failed', error: 'model download failed' })
    vi.mocked(api.installLocalAsr).mockRejectedValue(new Error('ASR_INSTALL_CONFLICT'))
    render(<Fixture />)
    expect(await screen.findByRole('alert')).toHaveTextContent('model download failed')
    await userEvent.click(screen.getByRole('button', { name: '重试安装' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('ASR_INSTALL_CONFLICT')
    expect(screen.getByRole('button', { name: '重试安装' })).toBeEnabled()
    await userEvent.click(screen.getByRole('switch', { name: '首次需要时自动安装' }))
    expect(savedSettings().workbenchMedia.localAsr.autoInstall).toBe(false)
    expect(savedSettings().workbenchMedia.transcribeModels).toEqual([{ providerId: 'local', model: 'whisperx-small' }])
  })
  it('shows an active installation and a busy transcription without offering to stop the active service', async () => {
    vi.mocked(api.getLocalAsrStatus).mockResolvedValue({ ...asrStatus, state: 'installing', operationId: 'install-1',
      progress: { stage: 'dependencies', message: 'Downloading fixed dependencies' }, runtime: { state: 'busy', pid: 100, activeTaskId: 'transcription-1' } })
    vi.mocked(api.cancelLocalAsrInstall).mockRejectedValue(new Error('ASR_INSTALL_CONFLICT: stale operation'))
    render(<Fixture />)
    await screen.findByText(/Downloading fixed dependencies/)
    expect(screen.getByRole('button', { name: '停止空闲服务' })).toBeDisabled()
    await userEvent.click(screen.getByRole('button', { name: '取消本次安装' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('ASR_INSTALL_CONFLICT')
    expect(screen.getByRole('button', { name: '手动安装／更新语言' })).toBeDisabled()
  })
  it('keeps a failed local reference deletion visible and removes it only after success', async () => {
    vi.mocked(api.listMediaVoices).mockResolvedValue([{ ...voiceReference, voiceId: 'voice_owned' }])
    vi.mocked(api.deleteMediaVoice).mockRejectedValueOnce(new Error('local reference is locked')).mockResolvedValueOnce(undefined)
    render(<Fixture />)
    await screen.findByText('voice_owned')
    await userEvent.click(screen.getByRole('button', { name: '删除本地引用' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('local reference is locked')
    expect(screen.getByText('voice_owned')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: '删除本地引用' }))
    await screen.findByText('已删除本地引用；未删除供应商声音或任务产物。')
    expect(screen.queryByText('voice_owned')).toBeNull()
  })
  it('does not resurrect a deleted local reference when an older refresh completes late', async () => {
    let finishRefresh!: (references: VoiceReference[]) => void
    vi.mocked(api.listMediaVoices).mockResolvedValueOnce([voiceReference]).mockImplementationOnce(() => new Promise(resolve => { finishRefresh = resolve }))
    vi.mocked(api.deleteMediaVoice).mockResolvedValue(undefined)
    render(<Fixture />)
    await screen.findByText('voice_fixture')
    await userEvent.click(screen.getByRole('button', { name: '刷新声音列表' }))
    await userEvent.click(screen.getByRole('button', { name: '删除本地引用' }))
    await screen.findByText('已删除本地引用；未删除供应商声音或任务产物。')
    await act(async () => { finishRefresh([voiceReference]) })
    expect(screen.queryByText('voice_fixture')).toBeNull()
  })
})
