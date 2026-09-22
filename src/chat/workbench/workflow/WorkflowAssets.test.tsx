/** @vitest-environment jsdom */
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import type { WorkflowAsset } from './workflowConfig'
import { WorkflowAssets } from './WorkflowAssets'
vi.mock('../../attachmentPreview', () => ({ loadAttachmentDataUrl: vi.fn().mockResolvedValue('data:image/png;base64,AA=='), openAttachment: vi.fn() }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../../../api/tauri', async () => ({ ...await vi.importActual('../../../api/tauri'), isTauriRuntime: () => true }))
function Host() {
  const [assets, setAssets] = useState<WorkflowAsset[]>([{ path: '/missing.png', name: '旧参考', description: '原说明' }])
  return <><WorkflowAssets assets={assets} onChange={setAssets} /><output>{JSON.stringify(assets)}</output></>
}
afterEach(() => vi.restoreAllMocks())
describe('durable workflow assets', () => {
  it('marks missing files, replaces the path, updates description and removes assets', async () => {
    vi.spyOn(api, 'chatInspectAttachmentPaths').mockImplementation(async paths => paths.includes('/new.png') ? [{ path: '/new.png', name: '新参考', type: 'image' }] : [])
    vi.mocked(open).mockResolvedValue('/new.png')
    render(<Host />)
    expect(await screen.findByText('素材未验证或不可访问，请重新选择')).toBeVisible()
    fireEvent.click(screen.getByText('更换素材'))
    await screen.findByAltText('新参考')
    fireEvent.change(screen.getByLabelText('素材描述 1'), { target: { value: '柔和光线' } })
    expect(screen.getByRole('status').textContent).toContain('/new.png')
    expect(screen.getByRole('status').textContent).toContain('柔和光线')
    fireEvent.click(screen.getByText('移除 新参考'))
    expect(screen.getByRole('status').textContent).toBe('[]')
  })
  it('ignores picker completion after node unmount', async () => {
    let finish!: (value: string) => void
    vi.mocked(open).mockImplementation(() => new Promise(resolve => { finish = resolve }))
    vi.spyOn(api, 'chatInspectAttachmentPaths').mockResolvedValue([])
    const changed = vi.fn(), view = render(<WorkflowAssets assets={[]} onChange={changed} />)
    fireEvent.click(screen.getByText('选择图片'))
    await waitFor(() => expect(screen.getByText('正在选择…')).toBeDisabled())
    view.unmount(); await act(async () => { finish('/late.png') })
    expect(changed).not.toHaveBeenCalled()
  })
})
