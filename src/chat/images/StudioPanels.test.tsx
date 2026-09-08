import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { getSettingsCached } from '../../api/settingsCache'
import { makeProvider } from '../../settings/tabs/testFixtures'
import { ConfigPanel } from './StudioPanels'

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
