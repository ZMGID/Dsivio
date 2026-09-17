import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { InputBar } from './InputBar'
import { draftKey, setComposerDraft } from './composerDraft'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onFocusChanged: () => Promise.resolve(() => {}) }),
}))
vi.mock('../api/tauri', () => ({ api: {}, isTauriRuntime: () => false }))
vi.mock('./api', () => ({ chatApi: { getProjects: () => Promise.resolve([]) } }))

function videoDraft(id: string) {
  setComposerDraft(draftKey(id), {
    input: '看看这个', quotes: [],
    attachments: [{ id: 'v1', type: 'video', name: 'clip.mp4', path: 'C:/clip.mp4' }],
  })
}

describe('video analysis requires a deliberate composer action', () => {
  it('普通发送视频不附加分析请求', async () => {
    videoDraft('video-normal')
    const onSend = vi.fn()
    render(<InputBar conversationId="video-normal" onSend={onSend} />)
    expect(onSend).not.toHaveBeenCalled()
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' })
    await waitFor(() => expect(onSend).toHaveBeenCalled())
    expect(onSend.mock.calls[0][0]).toBe('看看这个')
  })

  it('点击分析保留问题，发送后才执行，下一轮不沿用分析请求', async () => {
    videoDraft('video-explicit')
    const onSend = vi.fn()
    render(<InputBar conversationId="video-explicit" onSend={onSend} gitLang="zh" />)
    fireEvent.click(screen.getByRole('button', { name: '分析视频' }))
    expect(onSend).not.toHaveBeenCalled()
    expect(screen.getByRole('textbox')).toHaveValue('/video 看看这个')
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' })
    await waitFor(() => expect(screen.getByRole('textbox')).toHaveValue(''))
    expect(onSend.mock.calls[0][0]).toBe('/video 看看这个')
    fireEvent.change(screen.getByRole('textbox'), { target: { value: '继续解释' } })
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' })
    await waitFor(() => expect(onSend).toHaveBeenCalledTimes(2))
    expect(onSend.mock.calls[1][0]).toBe('继续解释')
  })

  it('有历史视频时可主动重新分析；关闭时入口禁用', () => {
    const props = { conversationId: 'video-history', onSend: vi.fn(), hasVideoHistory: true, gitLang: 'zh' as const }
    const { rerender } = render(<InputBar {...props} />)
    fireEvent.click(screen.getByRole('button', { name: '分析 / 重新分析视频' }))
    expect(screen.getByRole('textbox')).toHaveValue('/video ')
    rerender(<InputBar {...props} videoAnalysisEnabled={false} />)
    expect(screen.getByRole('button', { name: '分析 / 重新分析视频' })).toBeDisabled()
    expect(screen.getByText(/视频分析已关闭/)).toBeVisible()
  })

  it('外部 CLI 不展示混音器分析入口', () => {
    render(<InputBar conversationId="video-cli" onSend={vi.fn()} hasVideoHistory usesExternalRuntime />)
    expect(screen.queryByRole('button', { name: /分析.*视频/ })).toBeNull()
  })
})
