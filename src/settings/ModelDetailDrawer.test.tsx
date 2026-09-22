import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ModelDetailDrawer } from './ModelDetailDrawer'
import { api } from '../api/tauri'

vi.mock('../api/tauri', () => ({ api: { previewVideoModelRequest: vi.fn(), openExternal: vi.fn() } }))

describe('ModelDetailDrawer video configuration', () => {
  it('shows native video parameters and handles preview failure without submitting', async () => {
    vi.mocked(api.previewVideoModelRequest).mockRejectedValueOnce(new Error('preview unavailable'))
    const onClose = vi.fn()
    render(<ModelDetailDrawer modelName="MiniMax-H3" lang="zh" onClose={onClose} onSave={vi.fn()} onReset={vi.fn()} />)
    expect(screen.getByRole('button', { name: '视频接口协议' })).toHaveTextContent('MiniMax H3')
    expect(screen.queryByText('上下文长度')).toBeNull()
    expect(screen.queryByText('工具调用')).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: '预览请求（不提交）' }))
    expect(screen.getByRole('alert')).toHaveTextContent('preview unavailable')
    expect(api.previewVideoModelRequest).toHaveBeenCalledWith({ model: 'MiniMax-H3', protocol: 'minimax_h3', baseUrl: 'https://api.minimax.cn' })
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' })
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('requires an explicit protocol for a custom video model and saves it', async () => {
    const onSave = vi.fn()
    render(<ModelDetailDrawer modelName="custom-video" overrides={{ 'custom-video': { capabilities: { videoGeneration: true } } }} lang="zh" onClose={vi.fn()} onSave={onSave} onReset={vi.fn()} />)
    expect(screen.getByRole('button', { name: '保存' })).toBeDisabled()
    await userEvent.click(screen.getByRole('button', { name: '视频接口协议' }))
    await userEvent.click(screen.getByRole('option', { name: 'Volcengine Seedance' }))
    await userEvent.click(screen.getByRole('button', { name: '保存' }))
    expect(onSave).toHaveBeenCalledWith('custom-video', expect.objectContaining({ videoProtocol: 'seedance', capabilities: expect.objectContaining({ videoGeneration: true }) }))
  })
})
