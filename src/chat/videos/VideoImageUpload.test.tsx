import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { loadAttachmentDataUrl } from '../attachmentPreview'
import { VideoImageUpload } from './VideoImageUpload'

vi.mock('../../api/tauri', () => ({ isTauriRuntime: () => true }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../attachmentPreview', () => ({ loadAttachmentDataUrl: vi.fn() }))
let drop: (event: { payload: { type: string; position: { x: number; y: number }; paths: string[] } }) => void
const stop = vi.fn()
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: (fn: typeof drop) => { drop = fn; return Promise.resolve(stop) } }) }))
beforeEach(() => { vi.clearAllMocks() })

it('reads native selected images through the attachment reader and guards the shared submit while reading', async () => {
  vi.mocked(open).mockResolvedValue(['/tmp/product.png'])
  let finish!: (url: string) => void
  vi.mocked(loadAttachmentDataUrl).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const onChange = vi.fn(), onBusyChange = vi.fn(), onNotice = vi.fn()
  const view = render(<VideoImageUpload label="参考图片" files={[]} max={4} onChange={onChange} onNotice={onNotice} onBusyChange={onBusyChange} />)
  await userEvent.click(screen.getByRole('button', { name: '选择商品图片' }))
  expect(onBusyChange).toHaveBeenLastCalledWith(true)
  expect(screen.getByRole('button', { name: '正在读取…' })).toBeDisabled()
  expect(loadAttachmentDataUrl).toHaveBeenCalledWith({ path: '/tmp/product.png', name: 'product.png', type: 'image' })
  await act(async () => finish('data:image/png;base64,cGl4ZWw='))
  expect(onChange).toHaveBeenCalledWith([expect.objectContaining({ name: 'product.png', url: 'data:image/png;base64,cGl4ZWw=' })])
  expect(onBusyChange).toHaveBeenLastCalledWith(false)
  view.unmount()
  expect(stop).toHaveBeenCalled()
})

it('accepts native drops only over its own material area and reports unreadable files', async () => {
  vi.mocked(loadAttachmentDataUrl).mockResolvedValue(null)
  const onChange = vi.fn(), onNotice = vi.fn()
  const { container } = render(<VideoImageUpload label="参考图片" files={[]} max={4} onChange={onChange} onNotice={onNotice} />)
  const area = container.firstElementChild as HTMLElement
  vi.spyOn(area, 'getBoundingClientRect').mockReturnValue({ left: 0, top: 0, right: 100, bottom: 100 } as DOMRect)
  await act(async () => drop({ payload: { type: 'drop', position: { x: 500, y: 500 }, paths: ['/tmp/missing.png'] } }))
  expect(loadAttachmentDataUrl).not.toHaveBeenCalled()
  await act(async () => drop({ payload: { type: 'drop', position: { x: 10, y: 10 }, paths: ['/tmp/missing.png'] } }))
  await waitFor(() => expect(onNotice).toHaveBeenCalledWith(expect.stringContaining('无法读取图片')))
  expect(onChange).not.toHaveBeenCalled()
})
