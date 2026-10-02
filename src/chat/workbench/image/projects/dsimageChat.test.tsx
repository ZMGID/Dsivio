import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { draftKey, getComposerDraft, setComposerDraft } from '../../../composerDraft'
import { onRequestNewChat } from '../../../requestNewChat'
import { emptyBrief, type ImageBrief } from './types'
import { UseChatForSetButton } from './dsimageChat'
import { dsimageChatPrompt, isImageSetChatFeature } from './dsimageChatPrompt'
import ImageProjectWorkspace from './ImageProjectWorkspace'
import { CloneImageForm } from './CloneImageForm'
import { FreeImageForm } from './FreeImageForm'

vi.mock('../../../../api/tauri', () => ({
  isTauriRuntime: () => false,
  api: {
    studioTaskLibrary: vi.fn(async () => ({})),
    workbenchImageBootstrap: vi.fn(),
    workbenchImageGet: vi.fn(),
    workbenchImageSave: vi.fn(),
    workbenchImageAction: vi.fn(),
    workbenchImagePreview: vi.fn(),
    workbenchImageImport: vi.fn(),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}))

const brief: ImageBrief = {
  ...emptyBrief('design'),
  name: '夏季水杯',
  requirement: '突出杯盖和容量',
  language: 'zh-CN',
  platform: 'shopee',
  style: '干净',
  templateId: 'builtin-default-v1',
  products: [{
    id: 'p1',
    name: '杯',
    category: '杯壶',
    kind: '',
    facts: '350ml',
    front: '/img/front.png',
    back: '/img/back.png',
    assets: [
      { id: 'a', name: 'front', path: '/img/front.png' },
      { id: 'b', name: 'side', path: '/img/side.png' },
    ],
    templateId: null,
  }],
  workflowInput: { mode: 'smart', sources: [{ id: 's', name: '样图', path: '/img/example.png' }] },
}

afterEach(() => {
  cleanup()
  setComposerDraft(draftKey(null), { input: '', quotes: [], attachments: [] })
})

describe('用对话做', () => {
  it('prefill includes the dsimage skill, requirement, and product image paths', () => {
    const prompt = dsimageChatPrompt(brief)
    expect(prompt.startsWith('/dsimage\n')).toBe(true)
    expect(prompt).toContain('design（套图设计')
    expect(prompt).toContain('要求：突出杯盖和容量')
    expect(prompt).toContain('正面 /img/front.png')
    expect(prompt).toContain('背面 /img/back.png')
    expect(prompt).toContain('/img/side.png')
    expect(prompt).toContain('/img/example.png')
    expect(prompt).toContain('350ml')
    expect(prompt.split('/img/front.png')).toHaveLength(2)
    expect(isImageSetChatFeature('replace')).toBe(true)
    expect(isImageSetChatFeature('smart')).toBe(true)
    expect(isImageSetChatFeature('client')).toBe(true)
    expect(isImageSetChatFeature('gen')).toBe(false)
  })

  it('button writes that prompt into the new-chat draft and asks for a new conversation', () => {
    const requested = vi.fn()
    const stop = onRequestNewChat(requested)
    render(<UseChatForSetButton brief={brief} />)
    fireEvent.click(screen.getByRole('button', { name: '用对话做' }))
    expect(requested).toHaveBeenCalledOnce()
    expect(getComposerDraft(draftKey(null))?.input).toBe(dsimageChatPrompt(brief))
    expect(getComposerDraft(draftKey(null))?.quotes).toEqual([])
    stop()
  })

  it('shows the button on set pages and not on single-image pages', () => {
    const { unmount } = render(<ImageProjectWorkspace feature="replace" Form={CloneImageForm} />)
    expect(screen.getByRole('button', { name: '用对话做' })).toBeTruthy()
    unmount()
    render(<ImageProjectWorkspace feature="gen" Form={FreeImageForm} />)
    expect(screen.queryByRole('button', { name: '用对话做' })).toBeNull()
  })
})
