import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import VideoStudio from './VideoStudio'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    videoStudioBootstrap: vi.fn(async () => ({
      tasks: [], config: {}, root: '', configPath: '',
      dependencies: { python: '3.14', comfy: false, node: true, ffmpeg: true },
      templates: [{ id: 'chat-template', name: '聊天创建的参考模板', kind: 'reference', script: '真实参考镜头' }],
    })),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))

describe('shared video workspace navigation', () => {
  it('loads chat templates and starts a draft without silently choosing a generation route', async () => {
    render(<VideoStudio />)
    await screen.findByRole('button', { name: '模板库 1' })
    fireEvent.click(screen.getByRole('button', { name: '模板库 1' }))
    await screen.findByRole('heading', { name: '聊天创建的参考模板' })
    fireEvent.click(screen.getByRole('button', { name: '使用模板' }))
    expect(await screen.findByText('已选模板：聊天创建的参考模板')).toBeTruthy()
    expect(screen.getByRole('button', { name: '生成路线' }).textContent).toContain('请选择')
    fireEvent.click(screen.getByRole('button', { name: '视频设置' }))
    expect(await screen.findByRole('heading', { name: '运行环境' })).toBeTruthy()
  })
})
