import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { getSettingsCached } from '../../api/settingsCache'
import { makeProvider } from '../../settings/tabs/testFixtures'
import { ConfigPanel, ImageLanguageSelect } from './StudioPanels'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {},
  normalizeProviderApiFormat: (apiFormat?: string) => {
    if (apiFormat === 'gemini') return 'gemini'
    if (apiFormat === 'xai_responses') return 'xai_responses'
    return 'openai_chat'
  },
}))
vi.mock('../../api/settingsCache', () => ({
  getSettingsCached: vi.fn(),
}))

describe('Image language inheritance', () => {
  it('keeps the full template policy without showing it as a custom input', () => {
    const policy = '巴西葡萄牙语（pt-BR）。保留品牌名称。'
    const onChange = vi.fn()
    render(<ImageLanguageSelect value={policy} inheritedValue={policy} onChange={onChange} />)
    expect(screen.getByText('模板语言 · 葡萄牙语（巴西）')).toBeInTheDocument()
    expect(screen.queryByLabelText('自定义图内语言')).not.toBeInTheDocument()
    expect(onChange).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '图内语言' }))
    fireEvent.click(screen.getByRole('option', { name: '无文字' }))
    expect(onChange).toHaveBeenCalledWith('无文字')
  })
})

describe('ConfigPanel', () => {
  it('lists only image-generation models in the shared model picker', async () => {
    vi.mocked(getSettingsCached).mockResolvedValue({
      providers: [
        makeProvider({
          id: 'deepseek',
          name: 'DeepSeek',
          enabledModels: ['deepseek-v4-flash', 'deepseek-v4-pro'],
        }),
        makeProvider({
          id: 'openai',
          name: 'OpenAI',
          enabledModels: ['gpt-4o', 'gpt-image-1'],
        }),
      ],
    } as never)

    const onSave = vi.fn().mockResolvedValue(undefined)
    render(
      <ConfigPanel
        config={{
          providerId: '',
          model: '',
          protocol: 'openai',
          agentProviderId: '',
          agentModel: '',
        }}
        onSave={onSave}
        onClose={vi.fn()}
      />,
    )

    await waitFor(() => expect(screen.getByRole('button', { name: '图片模型' })).toBeEnabled())
    expect(screen.queryByText('图片接口协议')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '图片模型' }))
    expect(await screen.findByRole('option', { name: 'OpenAI - gpt-image-1' })).toBeInTheDocument()
    expect(screen.queryByRole('option', { name: /DeepSeek/ })).not.toBeInTheDocument()
    expect(screen.queryByRole('option', { name: /gpt-4o/ })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('option', { name: 'OpenAI - gpt-image-1' }))
    fireEvent.click(screen.getByRole('button', { name: '保存配置' }))
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith(
        expect.objectContaining({
          providerId: 'openai',
          model: 'gpt-image-1',
          protocol: 'openai',
        }),
      ),
    )
  })
})

it('updates an untouched image config when chat changes it', async () => {
  vi.mocked(getSettingsCached).mockResolvedValue({ providers: [] } as never)
  const config = { providerId: '', model: '', protocol: 'openai' as const, agentProviderId: '', agentModel: '', outputRoot: '/old' }
  const onSave = vi.fn().mockResolvedValue(undefined)
  const { rerender } = render(<ConfigPanel config={config} onSave={onSave} onClose={() => {}} />)
  rerender(<ConfigPanel config={{ ...config, outputRoot: '/chat-updated' }} onSave={onSave} onClose={() => {}} />)
  expect(await screen.findByDisplayValue('/chat-updated')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '保存配置' }))
  await waitFor(() => expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ outputRoot: '/chat-updated' })))
})

it('preserves a saved protocol when saving unrelated image settings', async () => {
  vi.mocked(getSettingsCached).mockResolvedValue({ providers: [] } as never)
  const config = { providerId: 'custom', model: 'custom-image', protocol: 'async', agentProviderId: '', agentModel: '', outputRoot: '/images' }
  const onSave = vi.fn().mockResolvedValue(undefined)
  render(<ConfigPanel config={config} onSave={onSave} onClose={() => {}} />)
  fireEvent.click(screen.getByRole('button', { name: '保存配置' }))
  await waitFor(() => expect(onSave).toHaveBeenCalledWith(config))
})
