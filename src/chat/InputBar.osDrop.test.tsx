import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { vi } from 'vitest'
import { InputBar } from './InputBar'
import { draftKey, setComposerDraft } from './composerDraft'

let dropHandler:
  | ((event: { payload: { type: string; paths?: string[] } }) => void)
  | undefined

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (fn: (event: { payload: { type: string; paths?: string[] } }) => void) => {
      dropHandler = fn
      return Promise.resolve(() => {
        if (dropHandler === fn) dropHandler = undefined
      })
    },
  }),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onFocusChanged: () => Promise.resolve(() => {}) }),
}))
vi.mock('../api/tauri', () => ({
  api: {
    chatClassifyAttachmentPaths: async (paths: string[]) =>
      paths.map((path) => ({
        path,
        name: path.split(/[/\\]/).pop() || path,
        kind: /\.(png|jpe?g|gif|webp|bmp|tiff?|heic|heif|mp4|mov)$/i.test(path) ? 'file' : 'directory',
      })),
  },
  isTauriRuntime: () => true,
}))
vi.mock('./api', () => ({
  chatApi: {
    getProjects: () => Promise.resolve([]),
    listExternalCliSlashCommands: () => Promise.resolve({ commands: [] }),
  },
}))

describe('InputBar OS drops', () => {
  beforeEach(() => {
    dropHandler = undefined
    setComposerDraft(draftKey(null), { input: '', quotes: [], attachments: [] })
  })

  it('ignores window drops when the conversation composer is hidden', async () => {
    render(<InputBar onSend={() => {}} acceptOsDrops={false} />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\goods\\shot.png'] } })
    expect(screen.queryByRole('button', { name: '移除' })).not.toBeInTheDocument()
  })

  it('attaches dropped images while the composer is the active surface', async () => {
    render(<InputBar onSend={() => {}} />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\goods\\shot.png'] } })
    expect(await screen.findByRole('button', { name: '移除' })).toBeInTheDocument()
  })

  it('attaches dropped folders as folder chips', async () => {
    render(<InputBar onSend={() => {}} />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'drop', paths: ['E:\\ZM database\\numao\\VE女包系列'] } })
    expect(await screen.findByText('FOLDER')).toBeInTheDocument()
    expect(screen.getByText('VE女包系列')).toBeInTheDocument()
  })
  it('sends mixed folder and video drops with their distinct attachment types', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'drop', paths: ['/goods/catalog', '/goods/demo.mp4'] } })
    expect(await screen.findByText('catalog')).toBeInTheDocument()
    expect(screen.getByText('demo.mp4')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '发送' }))
    await waitFor(() => expect(onSend).toHaveBeenCalled())
    expect(onSend.mock.calls[0][1]).toEqual([
      expect.objectContaining({ type: 'folder', path: '/goods/catalog' }),
      expect.objectContaining({ type: 'video', path: '/goods/demo.mp4' }),
    ])
  })

})
