import { act, render, screen } from '@testing-library/react'
import { useRef } from 'react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { RouteActiveContext } from '../chatRouteVisibility'
import { IMAGE_EXTENSIONS, VIDEO_EXTENSIONS, useFileDrop } from './useFileDrop'

type Payload =
  | { type: 'enter' | 'over'; position: { x: number; y: number }; paths?: string[] }
  | { type: 'drop'; position: { x: number; y: number }; paths: string[] }
  | { type: 'leave' }

const bridge = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: Payload }) => void),
  unlistened: 0,
}))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: typeof bridge.handler) => {
      bridge.handler = handler
      return Promise.resolve(() => {
        bridge.handler = null
        bridge.unlistened += 1
      })
    },
  }),
}))

function Zone({ extensions, onDrop }: { extensions: readonly string[]; onDrop: (accepted: string[], rejected: string[]) => void }) {
  const ref = useRef<HTMLDivElement>(null)
  const over = useFileDrop(ref, extensions, onDrop)
  return <div ref={ref} data-testid="zone" data-over={String(over)} />
}

function placeZone() {
  vi.spyOn(screen.getByTestId('zone'), 'getBoundingClientRect').mockReturnValue({
    left: 10, top: 10, right: 110, bottom: 110, width: 100, height: 100, x: 10, y: 10, toJSON: () => ({}),
  })
}

const send = (payload: Payload) => act(async () => { bridge.handler?.({ payload }) })
const inside = { x: 50, y: 50 }
const outside = { x: 500, y: 500 }

beforeEach(() => {
  bridge.handler = null
  bridge.unlistened = 0
  ;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
})
afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  vi.restoreAllMocks()
})

it('hands over only matching files that land inside the zone, case-insensitively', async () => {
  const onDrop = vi.fn()
  render(<Zone extensions={IMAGE_EXTENSIONS} onDrop={onDrop} />)
  placeZone()
  await act(async () => {})
  await send({ type: 'drop', position: inside, paths: ['/a/Backpack.PNG', '/a/notes.txt', '/a/folder'] })
  expect(onDrop).toHaveBeenCalledWith(['/a/Backpack.PNG'], ['/a/notes.txt', '/a/folder'])
})

it('ignores a drop that lands outside the zone', async () => {
  const onDrop = vi.fn()
  render(<Zone extensions={IMAGE_EXTENSIONS} onDrop={onDrop} />)
  placeZone()
  await act(async () => {})
  await send({ type: 'drop', position: outside, paths: ['/a/b.png'] })
  expect(onDrop).not.toHaveBeenCalled()
})

it('highlights only while hovering inside the zone and clears on leave and drop', async () => {
  render(<Zone extensions={VIDEO_EXTENSIONS} onDrop={vi.fn()} />)
  placeZone()
  await act(async () => {})
  const zone = screen.getByTestId('zone')
  await send({ type: 'over', position: inside })
  expect(zone).toHaveAttribute('data-over', 'true')
  await send({ type: 'over', position: outside })
  expect(zone).toHaveAttribute('data-over', 'false')
  await send({ type: 'over', position: inside })
  await send({ type: 'leave' })
  expect(zone).toHaveAttribute('data-over', 'false')
  await send({ type: 'over', position: inside })
  await send({ type: 'drop', position: inside, paths: ['/a/clip.mp4'] })
  expect(zone).toHaveAttribute('data-over', 'false')
})

it('listens only while its route is in the foreground and releases the listener on unmount', async () => {
  const onDrop = vi.fn()
  const page = (active: boolean) => (
    <RouteActiveContext.Provider value={active}>
      <Zone extensions={IMAGE_EXTENSIONS} onDrop={onDrop} />
    </RouteActiveContext.Provider>
  )
  const view = render(page(false))
  await act(async () => {})
  expect(bridge.handler).toBeNull()

  view.rerender(page(true))
  placeZone()
  await act(async () => {})
  expect(bridge.handler).not.toBeNull()

  view.rerender(page(false))
  await act(async () => {})
  expect(bridge.handler).toBeNull()

  view.rerender(page(true))
  await act(async () => {})
  view.unmount()
  await act(async () => {})
  expect(bridge.handler).toBeNull()
})

it('does not listen outside the desktop app', async () => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__
  render(<Zone extensions={IMAGE_EXTENSIONS} onDrop={vi.fn()} />)
  await act(async () => {})
  expect(bridge.handler).toBeNull()
})
