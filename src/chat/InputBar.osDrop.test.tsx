import { render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { vi } from 'vitest'
import { InputBar } from './InputBar'

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
vi.mock('../api/tauri', () => ({ api: {}, isTauriRuntime: () => true }))
vi.mock('./api', () => ({
  chatApi: {
    getProjects: () => Promise.resolve([]),
    listExternalCliSlashCommands: () => Promise.resolve({ commands: [] }),
  },
}))

describe('InputBar OS drops', () => {
  beforeEach(() => {
    dropHandler = undefined
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
})
