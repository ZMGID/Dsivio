import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { useMediaGeneration } from './useMediaGeneration'
import type { MediaRequest } from '../../generated/mediaGeneration'
const request: MediaRequest = { providerId: 'p', model: 'm', kind: 'image', prompt: 'product', images: [], options: {}, origin: 'workbench/edit' }
beforeEach(() => { vi.restoreAllMocks(); vi.spyOn(api, 'listMediaTasks').mockResolvedValue([]); vi.spyOn(api, 'startMediaGeneration').mockResolvedValue({ id: 'task', providerId: 'p', model: 'm', kind: 'image', prompt: '', origin: request.origin, createdAt: '', status: 'running', remoteId: null, error: null, outputs: [], canResume: false }) })
it('locks submission while reading files and submits only once', async () => {
 const page = renderHook(() => useMediaGeneration({ origin: request.origin }))
 await waitFor(() => expect(page.result.current.loading).toBe(false))
 let finish!: (value: MediaRequest) => void
 const readFiles = vi.fn(() => new Promise<MediaRequest>(resolve => { finish = resolve }))
 let pending!: Promise<unknown>
 act(() => { pending = page.result.current.submit(readFiles); void page.result.current.submit(readFiles) })
 expect(readFiles).toHaveBeenCalledTimes(1); expect(page.result.current.busy).toBe(true)
 await act(async () => { finish(request); await pending })
 expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
})
it('does not submit a late file read after leaving the feature', async () => {
 const page = renderHook(({ origin }) => useMediaGeneration({ origin }), { initialProps: { origin: 'workbench/edit' } })
 await waitFor(() => expect(page.result.current.loading).toBe(false))
 let finish!: (value: MediaRequest) => void
 let pending!: Promise<unknown>
 act(() => { pending = page.result.current.submit(() => new Promise(resolve => { finish = resolve })) })
 page.rerender({ origin: 'workbench/poster' })
 await act(async () => { finish(request); await pending })
 expect(api.startMediaGeneration).not.toHaveBeenCalled()
})
