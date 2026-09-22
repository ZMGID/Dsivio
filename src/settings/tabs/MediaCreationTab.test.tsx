import { useState } from 'react'
import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import type { Settings } from '../../api/tauri'
import { MediaCreationTab } from './MediaCreationTab'
import { makeProvider, makeSettings } from './testFixtures'

const imageModels = ['image-a', 'image-b', 'image-c']
function Fixture({ initial = makeSettings({ providers: [makeProvider({
  enabledModels: [...imageModels, 'MiniMax-H3', 'grok-imagine-video', 'chat-only'],
  modelOverrides: Object.fromEntries(imageModels.map(model => [model, { capabilities: { imageGeneration: true } }])),
})] }) }: { initial?: Settings }) {
  const [settings, setSettings] = useState(initial)
  return <><MediaCreationTab settings={settings} lang="zh" onUpdatePool={(kind, models) => setSettings(s => ({ ...s, workbenchMedia: { ...s.workbenchMedia, [kind]: models } }))} /><output data-testid="settings">{JSON.stringify(settings)}</output></>
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
    expect(screen.getAllByRole('switch')).toHaveLength(2)
    expect(screen.getByRole('switch', { name: `图片供应商 / ${models[0]}` }).getAttribute('aria-checked')).toBe('true')
    await userEvent.clear(search)
    await userEvent.type(search, 'no-match')
    expect(screen.getByText('没有匹配的模型')).toBeTruthy()
    expect(savedSettings().workbenchMedia.imageModels).toHaveLength(1)
  })

  it('keeps unavailable members visible and removable without deleting their provider', async () => {
    render(<Fixture initial={makeSettings({ providers: [makeProvider({ enabled: false, enabledModels: ['image-a'] })],
      workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'image-a' }], videoModels: [] },
    })} />)
    expect(screen.getByText(/当前不可用/)).toBeTruthy()
    await userEvent.click(screen.getByRole('switch', { name: 'OpenAI / image-a' }))
    expect(savedSettings().workbenchMedia.imageModels).toEqual([])
    expect(savedSettings().providers).toHaveLength(1)
    expect(screen.getAllByText(/请先在「模型」中/)).toHaveLength(2)
  })
})
