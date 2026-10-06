import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../api/tauri'
import { getSettingsCached, subscribeSettings } from '../../api/settingsCache'
import { makeProvider, makeSettings } from '../../settings/tabs/testFixtures'
import type { MediaModelInfo, MediaRequest, MediaTask } from '../../generated/mediaGeneration'
import { MediaStation, resetMediaWindowForTests } from './MediaStation'
import { useMediaGeneration } from '../workbench/useMediaGeneration'
import { buildStationRequest, toMediaJob, mediaStationApi } from './mediaStationAdapter'
vi.mock('../../api/tauri', async importOriginal => ({ ...await importOriginal<typeof import('../../api/tauri')>(), isTauriRuntime: () => true }))
vi.mock('../../api/settingsCache', () => ({ getSettingsCached: vi.fn(), subscribeSettings: vi.fn() }))
const task: MediaTask = { id: 't1', providerId: 'p1', model: 'gpt-image-1', kind: 'image', status: 'succeeded', createdAt: '2026-10-07T00:00:00Z', prompt: 'original', origin: 'media-station', error: null, remoteId: null, outputs: [], canResume: false, result: null, requestHash: null, cancellation: null }
const info: MediaModelInfo = { revision: 'r1', complete: true, parameters: [{ key: 'prompt', dataType: 'string', required: true, facts: {} }, { key: 'aspectRatio', dataType: 'string', required: false, facts: { allowed: ['1:1', '16:9'] } }, { key: 'images', dataType: 'mediaList', required: false, facts: { maxCount: 4 } }] }
const request: MediaRequest = { providerId: 'p1', model: 'gpt-image-1', kind: 'image', prompt: 'original', images: [], options: { aspectRatio: '16:9' }, origin: 'media-station' }
const page = () => <MediaStation onOpenSettings={vi.fn()} />
beforeEach(() => {
  vi.restoreAllMocks(); resetMediaWindowForTests()
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute('open', '') }
  HTMLDialogElement.prototype.close = function () { this.removeAttribute('open') }
  vi.mocked(subscribeSettings).mockReturnValue(() => {})
  vi.mocked(getSettingsCached).mockResolvedValue(makeSettings({ providers: [makeProvider({ enabledModels: ['gpt-image-1', 'gpt-image-2'], modelOverrides: { 'gpt-image-1': { capabilities: { imageGeneration: true } }, 'gpt-image-2': { capabilities: { imageGeneration: true } } } })], workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'gpt-image-1' }, { providerId: 'p1', model: 'gpt-image-2' }], videoModels: [] } }))
  vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
  vi.spyOn(api, 'describeMediaModel').mockResolvedValue(info)
  vi.spyOn(api, 'startMediaGeneration').mockResolvedValue(task)
  vi.spyOn(api, 'mediaTaskRequest').mockResolvedValue(request)
  const scope = renderHook(() => useMediaGeneration({ origin: 'media-station' }))
  act(() => scope.result.current.setError(''))
  scope.unmount()
  vi.mocked(api.listMediaTasks).mockClear()
})
afterEach(cleanup)
it('retains the upstream dock, four ideas, separate history and details layout', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([task])
  const { container } = render(page())
  expect(container.querySelectorAll('.kv-media-idea')).toHaveLength(4)
  expect(container.querySelector('.kv-media-dock .kv-media-composer')).toBeTruthy()
  expect(screen.queryByText('original')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: /创作记录/ }))
  await userEvent.click(await screen.findByRole('button', { name: /original/ }))
  expect(screen.getByRole('dialog', { name: '作品详情' })).toBeVisible()
  expect(container.querySelector('.kv-media-dock')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: '复用参数' }))
  expect(screen.getByLabelText('创作描述')).toHaveValue('original')
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
})
it('submits through the unified service and keeps the draft across navigation', async () => {
  const view = render(page())
  await waitFor(() => expect(api.describeMediaModel).toHaveBeenCalled())
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'a bag' } })
  view.unmount(); render(page())
  expect(screen.getByLabelText('创作描述')).toHaveValue('a bag')
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  expect(api.startMediaGeneration).toHaveBeenCalledWith(expect.objectContaining({ prompt: 'a bag', origin: 'media-station', descriptionRevision: 'r1', options: { aspectRatio: '1:1' } }))
})
it('locks an in-flight request and restores the latest result after navigation', async () => {
  let finish!: (task: MediaTask) => void
  vi.mocked(api.startMediaGeneration).mockImplementation(() => new Promise(resolve => { finish = resolve }))
  const view = render(page())
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'hold' } })
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  view.unmount(); render(page())
  expect(screen.getByRole('button', { name: '开始生成' })).toBeDisabled()
  vi.mocked(api.listMediaTasks).mockResolvedValue([task])
  await act(async () => { finish(task) })
  expect(await screen.findByRole('article', { name: '本次生成' })).toBeVisible()
  expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
})
it('changes models without submitting or losing the prompt', async () => {
  render(page())
  await waitFor(() => expect(screen.getByLabelText('模型')).toHaveTextContent('gpt-image-1'))
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'compare models' } })
  await userEvent.click(screen.getByLabelText('模型'))
  await userEvent.click(screen.getByRole('option', { name: 'gpt-image-2' }))
  await waitFor(() => expect(api.describeMediaModel).toHaveBeenLastCalledWith('p1', 'gpt-image-2', 'image'))
  expect(screen.getByLabelText('创作描述')).toHaveValue('compare models')
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
})
it('keeps history accessible with no configured model', async () => {
  vi.mocked(getSettingsCached).mockResolvedValue(makeSettings())
  vi.mocked(api.listMediaTasks).mockResolvedValue([task])
  render(page())
  await userEvent.click(screen.getByRole('button', { name: /创作记录/ }))
  expect(await screen.findByText('original')).toBeVisible()
})
it('keeps uncertain submission failures visible after leaving and does not retry', async () => {
  let fail!: (e: Error) => void
  vi.mocked(api.startMediaGeneration).mockImplementation(() => new Promise((_, reject) => { fail = reject }))
  const view = render(page())
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'hold' } })
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  view.unmount()
  await act(async () => { fail(new Error('uncertain')) })
  render(page())
  expect(await screen.findByRole('alert')).toHaveTextContent('uncertain')
  expect(api.startMediaGeneration).toHaveBeenCalledTimes(1)
})
it('maps video references, saved options and current description revision to the unified request', async () => {
  const videoInfo = { ...info, parameters: [{ key: 'prompt', dataType: 'string', required: true, facts: {} }, ...['ratio', 'duration', 'firstFrame', 'resolution'].map(key => ({ key, dataType: 'string', required: false, facts: {} }))] }
  expect(await buildStationRequest({ kind: 'video', providerId: 'p', model: 'v', prompt: 'move', aspectRatio: '16:9', duration: 2, referencePaths: ['/tmp/frame.jpg'], options: { resolution: '480p' } }, videoInfo)).toEqual({ kind: 'video', providerId: 'p', model: 'v', prompt: 'move', images: [], options: { resolution: '480p', ratio: '16:9', duration: 2, firstFrame: ['/tmp/frame.jpg'] }, descriptionRevision: 'r1', origin: 'media-station' })
  await expect(buildStationRequest({ kind: 'video', providerId: 'p', model: 'v', prompt: 'move', aspectRatio: '', duration: 0, referencePaths: ['/tmp/frame.jpg'] }, info)).rejects.toThrow('不支持')
})
it('only offers resume when the unified task can resume', async () => {
  expect((await toMediaJob({ ...task, status: 'failed', remoteId: 'remote', canResume: false })).providerTaskId).toBeUndefined()
  expect((await toMediaJob({ ...task, status: 'failed', remoteId: 'remote', canResume: true })).providerTaskId).toBe('remote')
})
it('keeps partial delete failures and exports the selected output index', async () => {
  vi.spyOn(api, 'deleteMediaTask').mockImplementation(async id => { if (id === 'blocked') throw new Error('running') })
  vi.spyOn(api, 'exportMediaOutput').mockResolvedValue('/tmp/output.jpg')
  expect(await mediaStationApi.delete(['removed', 'blocked'])).toEqual({ deletedIds: ['removed'], failures: [{ id: 'blocked', error: 'Error: running' }] })
  await mediaStationApi.export('t1', 1, '/tmp/output.jpg')
  expect(api.exportMediaOutput).toHaveBeenCalledWith('t1', '/tmp/output.jpg', 1)
})

it('selecting model defaults removes previously reused ratio and duration', async () => {
  const videoInfo = { ...info, parameters: ['prompt', 'ratio', 'duration'].map(key => ({ key, dataType: 'string', required: false, facts: {} })) }
  const next = await buildStationRequest({ kind: 'video', providerId: 'p', model: 'v', prompt: 'new', aspectRatio: '', duration: 0, referencePaths: [], options: { ratio: '16:9', duration: 10 } }, videoInfo)
  expect(next.options).toEqual({})
})
it('cancellation failures remain visible after history refresh', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([{ ...task, id: 'cancel-review', status: 'running' }])
  vi.spyOn(api, 'cancelMediaTask').mockRejectedValue(new Error('cancel endpoint unreachable'))
  render(page())
  await userEvent.click(screen.getByRole('button', { name: /创作记录/ }))
  await userEvent.click(await screen.findByRole('button', { name: /original/ }))
  await userEvent.click(screen.getByRole('button', { name: '取消生成' }))
  await waitFor(() => expect(vi.mocked(api.listMediaTasks).mock.calls.length).toBeGreaterThan(1))
  expect(screen.getByRole('alert')).toHaveTextContent('cancel endpoint unreachable')
})
it('unbound Comfy workflows must not silently discard an entered prompt', async () => {
  const provider = makeProvider({ enabledModels: ['flow'], request: { comfy: { workflows: [{ id: 'flow', name: 'Node inputs', kind: 'image', graph: { '3': { class_type: 'CLIPTextEncode', inputs: { text: 'old graph prompt' } } }, inputs: [{ nodeId: '3', input: 'text', label: 'Positive prompt', kind: 'text' }], outputNodes: ['9'] }] } } })
  vi.mocked(getSettingsCached).mockResolvedValue(makeSettings({ providers: [provider], workbenchMedia: { imageModels: [{ providerId: 'p1', model: 'flow' }], videoModels: [] } }))
  vi.mocked(api.describeMediaModel).mockResolvedValue({ revision: 'flow-revision', complete: false, parameters: [{ key: '3:text', dataType: 'string', required: false, facts: {} }] })
  render(page())
  await waitFor(() => expect(screen.getByLabelText('模型')).toHaveTextContent('flow'))
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'brand new prompt' } })
  expect(await screen.findByRole('alert')).toHaveTextContent('未绑定通用提示词')
  expect(screen.getByRole('button', { name: '开始生成' })).toBeDisabled()
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
  await expect(buildStationRequest({ kind: 'image', providerId: 'p1', model: 'flow', prompt: 'brand new prompt', aspectRatio: '', duration: 0, referencePaths: [] })).rejects.toThrow('未绑定通用提示词')
})
it('a failed model description read has a retry action without discarding the draft', async () => {
  vi.mocked(api.describeMediaModel).mockRejectedValue(new Error('temporary read error'))
  render(page())
  expect(await screen.findByRole('alert')).toHaveTextContent('temporary read error')
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'keep my draft' } })
  vi.mocked(api.describeMediaModel).mockResolvedValue(info)
  await userEvent.click(screen.getByRole('button', { name: '重试' }))
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
  expect(screen.queryByRole('alert')).toBeNull()
  expect(screen.getByLabelText('创作描述')).toHaveValue('keep my draft')
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
})

it('keeps resume failures visible and clears them after an explicit successful retry', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([{ ...task, id: 'resume-review', status: 'failed', canResume: true, remoteId: 'receipt' }])
  vi.spyOn(api, 'getMediaTask').mockRejectedValue(new Error('result endpoint unreachable'))
  render(page())
  await userEvent.click(screen.getByRole('button', { name: /创作记录/ }))
  await userEvent.click(await screen.findByRole('button', { name: /original/ }))
  await userEvent.click(screen.getByRole('button', { name: '继续获取结果' }))
  await waitFor(() => expect(vi.mocked(api.listMediaTasks).mock.calls.length).toBeGreaterThan(1))
  expect(screen.getByRole('alert')).toHaveTextContent('result endpoint unreachable')
  vi.mocked(api.getMediaTask).mockResolvedValue(task)
  await userEvent.click(screen.getByRole('button', { name: '继续获取结果' }))
  await waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
})
it('removes a reused image ratio when selecting the default while preserving other settings', async () => {
  const result = await buildStationRequest({ kind: 'image', providerId: 'p', model: 'm', prompt: 'new', aspectRatio: '', duration: 0, referencePaths: [], options: { aspectRatio: '16:9', quality: 'high' } }, info)
  expect(result.options).toEqual({ quality: 'high' })
})

it('ignores a late model-parameter retry after switching to another model', async () => {
  vi.mocked(api.describeMediaModel).mockRejectedValueOnce(new Error('retry me'))
  render(page())
  await screen.findByRole('button', { name: '重试' })
  let finish!: (info: MediaModelInfo) => void
  vi.mocked(api.describeMediaModel).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
  await userEvent.click(screen.getByRole('button', { name: '重试' }))
  fireEvent.change(screen.getByLabelText('创作描述'), { target: { value: 'keep description' } })
  expect(screen.getByRole('button', { name: '开始生成' })).toBeDisabled()
  await userEvent.click(screen.getByLabelText('模型'))
  await userEvent.click(screen.getByRole('option', { name: 'gpt-image-2' }))
  await waitFor(() => expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled())
  await act(async () => finish({ revision: 'stale', complete: false, parameters: [] }))
  expect(screen.getByRole('button', { name: '开始生成' })).toBeEnabled()
  expect(screen.queryByRole('alert')).toBeNull()
  await userEvent.click(screen.getByRole('button', { name: '开始生成' }))
  expect(api.startMediaGeneration).toHaveBeenCalledWith(expect.objectContaining({ model: 'gpt-image-2', prompt: 'keep description', descriptionRevision: 'r1' }))
})
