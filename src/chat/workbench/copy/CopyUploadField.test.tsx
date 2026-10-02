import { act, render, screen, waitFor } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { i18n } from '../../../components/i18n'
import type { LocalImage } from '../localMedia'
import { CopyUploadField } from './CopyUploadField'

const bridge = vi.hoisted(() => ({ handler: null as null | ((event: { payload: unknown }) => void) }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: typeof bridge.handler) => {
      bridge.handler = handler
      return Promise.resolve(() => { bridge.handler = null })
    },
  }),
}))
vi.mock('../../../api/tauri', () => ({ api: { workbenchReadLocalImage: vi.fn() } }))

const t = i18n.zh

function Harness({ initial = [], max, onNotice }: { initial?: LocalImage[]; max?: number; onNotice: (text: string) => void }) {
  const [files, setFiles] = useState<LocalImage[]>(initial)
  return (
    <>
      <CopyUploadField label="商品图" files={files} max={max} onChange={setFiles} onNotice={onNotice} />
      <output data-testid="names">{files.map((file) => file.name).join(',')}</output>
    </>
  )
}

const drop = (paths: string[]) => act(async () => {
  bridge.handler?.({ payload: { type: 'drop', position: { x: 5, y: 5 }, paths } })
})

beforeEach(() => {
  bridge.handler = null
  ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
  vi.mocked(api.workbenchReadLocalImage).mockReset()
  vi.mocked(api.workbenchReadLocalImage).mockImplementation(async (path: string) => ({
    name: path.split('/').pop() ?? '',
    mime: 'image/png',
    base64: 'AQID',
  }))
  vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['x'], { type: 'image/png' }) })))
  URL.createObjectURL = vi.fn(() => 'blob:preview')
  URL.revokeObjectURL = vi.fn()
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
    left: 0, top: 0, right: 400, bottom: 400, width: 400, height: 400, x: 0, y: 0, toJSON: () => ({}),
  })
})
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it('adds dropped images as previews and reports skipped file types', async () => {
  const onNotice = vi.fn()
  render(<Harness onNotice={onNotice} />)
  await act(async () => {})
  await drop(['/shots/front.png', '/shots/side.JPG', '/shots/readme.txt'])
  expect(screen.getByTestId('names')).toHaveTextContent('front.png,side.JPG')
  expect(api.workbenchReadLocalImage).toHaveBeenCalledTimes(2)
  expect(onNotice).toHaveBeenLastCalledWith(expect.stringContaining(t.workbenchDropUnsupported))
})

it('stops at the image limit without reading the extra files', async () => {
  const onNotice = vi.fn()
  render(<Harness max={2} initial={[{ id: 'a', name: 'old.png', url: 'blob:old' }]} onNotice={onNotice} />)
  await act(async () => {})
  await drop(['/shots/one.png', '/shots/two.png', '/shots/three.png'])
  expect(screen.getByTestId('names')).toHaveTextContent('old.png,one.png')
  expect(api.workbenchReadLocalImage).toHaveBeenCalledTimes(1)
  expect(onNotice).toHaveBeenLastCalledWith(t.workbenchCopyMaxFiles)
})

it('keeps the images that could be read and shows why one could not', async () => {
  vi.mocked(api.workbenchReadLocalImage).mockImplementation(async (path: string) => {
    if (path.endsWith('big.png')) throw '图片超过 10MB'
    return { name: path.split('/').pop() ?? '', mime: 'image/png', base64: 'AQID' }
  })
  const onNotice = vi.fn()
  render(<Harness onNotice={onNotice} />)
  await act(async () => {})
  await drop(['/shots/big.png', '/shots/ok.png'])
  expect(screen.getByTestId('names')).toHaveTextContent('ok.png')
  expect(onNotice).toHaveBeenLastCalledWith(`${t.workbenchDropFailed}图片超过 10MB`)
})

it('releases previews that finish loading after the page is left', async () => {
  const gate = Promise.withResolvers<{ name: string; mime: string; base64: string }>()
  vi.mocked(api.workbenchReadLocalImage).mockImplementation(() => gate.promise)
  const onNotice = vi.fn()
  const view = render(<Harness onNotice={onNotice} />)
  await act(async () => {})
  const dropping = drop(['/shots/late.png'])
  view.unmount()
  gate.resolve({ name: 'late.png', mime: 'image/png', base64: 'AQID' })
  await dropping
  await waitFor(() => expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:preview'))
})
