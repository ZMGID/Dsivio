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
  it('marks missing files, merges a picked file, updates description and removes assets', async () => {
    vi.spyOn(api, 'chatInspectAttachmentPaths').mockImplementation(async paths => paths.includes('/new.png') ? [{ path: '/new.png', name: '新参考', type: 'image' }] : [])
    vi.mocked(open).mockResolvedValue('/new.png')
    render(<Host />)
    expect(await screen.findByText('素材未验证或不可访问，请重新选择')).toBeVisible()
    fireEvent.click(screen.getByText('更换素材'))
    await screen.findByAltText('新参考')
    fireEvent.change(screen.getByLabelText('素材描述 1'), { target: { value: '柔和光线' } })
    const saved = () => document.querySelector('output')!.textContent!
    expect(saved()).toContain('/missing.png')
    expect(saved()).toContain('/new.png')
    expect(saved()).toContain('柔和光线')
    fireEvent.click(screen.getByText('移除 新参考'))
    fireEvent.click(screen.getByText('移除 旧参考'))
    expect(saved()).toBe('[]')
  })
  it('keeps a description edited while the file dialog is open and adds the picked asset', async () => {
    let finish!: (value: string[]) => void
    vi.mocked(open).mockImplementation(() => new Promise(resolve => { finish = resolve }))
    vi.spyOn(api, 'chatInspectAttachmentPaths').mockImplementation(async paths => paths.map(path => ({
      path, name: path === '/new.png' ? '新参考' : '旧参考', type: 'image' as const,
    })))
    function Host() {
      const [assets, setAssets] = useState<WorkflowAsset[]>([
        { path: '/old.png', name: '旧参考', description: '原说明' },
        { path: '/drop.png', name: '待移除', description: '临时' },
      ])
      return <>
        <button onClick={() => setAssets(current => current.filter(asset => asset.path !== '/drop.png'))}>外部移除</button>
        <WorkflowAssets assets={assets} onChange={setAssets} />
        <output data-testid="assets">{JSON.stringify(assets)}</output>
      </>
    }
    render(<Host />)
    fireEvent.click(screen.getByText('更换素材'))
    fireEvent.change(screen.getByLabelText('素材描述 1'), { target: { value: '改过的说明' } })
    fireEvent.click(screen.getByText('外部移除'))
    await act(async () => { finish(['/drop.png', '/new.png']) })
    await waitFor(() => expect(JSON.parse(screen.getByTestId('assets').textContent!)).toEqual([
      { path: '/old.png', name: '旧参考', description: '改过的说明' },
      { path: '/new.png', name: '新参考', description: '' },
    ]))
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
