import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api, isTauriRuntime } from '../../../api/tauri'
import type { PublishAccount, PublishRecord, PublishSubmitResult, VideoStats } from '../../../generated/publish'
import { AccountsPage } from './AccountsPage'
import { PublishDataPage } from './PublishDataPage'
import { PublishLogsPage } from './PublishLogsPage'
import { PublishPage } from './PublishPage'
import { buildPublishRequest } from './publishRequest'

vi.mock('../../../api/tauri', () => ({
  api: {
    publishListAccounts: vi.fn(async () => []),
    publishListRecords: vi.fn(async () => []),
    publishSubmit: vi.fn(),
    publishRetry: vi.fn(),
    publishRefreshRecord: vi.fn(),
    publishStats: vi.fn(),
    publishBegin: vi.fn(),
    publishComplete: vi.fn(),
    publishRefreshAccount: vi.fn(),
    publishUnbind: vi.fn(),
    openExternal: vi.fn(),
    listMediaTasks: vi.fn(async () => []),
  },
  isTauriRuntime: vi.fn(() => true),
}))

const account: PublishAccount = {
  id: 'acct-1', platform: 'youtube', remoteId: 'UC1', name: '频道甲',
  boundAt: '2026-01-01T00:00:00Z', checkedAt: '2026-01-02T00:00:00Z', status: 'connected', detail: null, fans: 12,
}

function record(patch: Partial<PublishRecord> = {}): PublishRecord {
  return {
    id: 'rec-1', groupId: 'g1', accountId: 'acct-1', platform: 'youtube', videoPath: '/tmp/clip.mp4',
    title: '一只猫', description: '说明', privacy: 'public', tags: ['cat'], status: 'failed',
    remoteId: null, url: null, reason: '上传失败', attempts: 1, createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z', origin: 'workbench/publish', mediaTaskId: null, ...patch,
  }
}

beforeEach(() => {
  vi.mocked(isTauriRuntime).mockReturnValue(true)
  vi.mocked(api.publishListAccounts).mockResolvedValue([account])
  vi.mocked(api.publishListRecords).mockResolvedValue([])
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  vi.mocked(api.publishSubmit).mockReset()
  vi.mocked(api.publishRetry).mockReset()
  vi.mocked(api.publishStats).mockReset()
  HTMLDialogElement.prototype.showModal = function showModal() { this.setAttribute('open', '') }
})

it('assembles a publish request from the form', async () => {
  const expected = buildPublishRequest({
    accountIds: ['acct-1'], videoPath: '/tmp/clip.mp4', title: '一只猫', description: '说明', privacy: 'public', tags: '#cat, dog',
  })
  expect(expected).toEqual({
    accountIds: ['acct-1'], videoPath: '/tmp/clip.mp4', title: '一只猫', description: '说明', privacy: 'public', tags: ['cat', 'dog'],
  })
  vi.mocked(api.publishSubmit).mockResolvedValue({ records: [record({ status: 'published', reason: null })], blocked: [] })
  render(<PublishPage />)
  fireEvent.click(await screen.findByRole('button', { name: '频道甲' }))
  fireEvent.change(screen.getByLabelText('视频路径'), { target: { value: '/tmp/clip.mp4' } })
  fireEvent.change(screen.getByLabelText('标题'), { target: { value: '一只猫' } })
  fireEvent.change(screen.getByPlaceholderText('填写视频描述'), { target: { value: '说明' } })
  fireEvent.change(screen.getByLabelText('标签'), { target: { value: '#cat, dog' } })
  fireEvent.click(screen.getByRole('button', { name: '发布' }))
  await waitFor(() => expect(api.publishSubmit).toHaveBeenCalledWith(expected))
})

it('shows a failed submit without inventing a record', async () => {
  vi.mocked(api.publishSubmit).mockRejectedValue(new Error('网络中断'))
  render(<PublishPage />)
  fireEvent.click(await screen.findByRole('button', { name: '频道甲' }))
  fireEvent.change(screen.getByLabelText('视频路径'), { target: { value: '/tmp/clip.mp4' } })
  fireEvent.change(screen.getByLabelText('标题'), { target: { value: '一只猫' } })
  fireEvent.click(screen.getByRole('button', { name: '发布' }))
  expect(await screen.findByText('网络中断')).toBeInTheDocument()
  expect(screen.queryByText('已发布')).not.toBeInTheDocument()
})

it('retries a failed record from the log', async () => {
  vi.mocked(api.publishListRecords).mockResolvedValue([record()])
  vi.mocked(api.publishRetry).mockResolvedValue(record({ status: 'published', reason: null, attempts: 2 }))
  render(<PublishLogsPage />)
  fireEvent.click(await screen.findByRole('button', { name: '重试' }))
  await waitFor(() => expect(api.publishRetry).toHaveBeenCalledWith('rec-1'))
  expect(screen.getByRole('cell', { name: '已发布' })).toBeInTheDocument()
})

it('ignores a submit that resolves after the page is left', async () => {
  let resolve!: (value: PublishSubmitResult) => void
  vi.mocked(api.publishSubmit).mockImplementation(() => new Promise((done) => { resolve = done }))
  const first = render(<PublishPage />)
  fireEvent.click(await screen.findByRole('button', { name: '频道甲' }))
  fireEvent.change(screen.getByLabelText('视频路径'), { target: { value: '/tmp/clip.mp4' } })
  fireEvent.change(screen.getByLabelText('标题'), { target: { value: '一只猫' } })
  fireEvent.click(screen.getByRole('button', { name: '发布' }))
  await waitFor(() => expect(api.publishSubmit).toHaveBeenCalledTimes(1))
  first.unmount()
  render(<PublishPage />)
  await act(async () => { resolve({ records: [record({ title: '迟到的结果', status: 'published' })], blocked: [] }) })
  expect(screen.queryByText(/迟到的结果/)).not.toBeInTheDocument()
})

it('reloads records when the log is opened again', async () => {
  vi.mocked(api.publishListRecords).mockResolvedValue([record({ status: 'published', title: '第一次' })])
  const first = render(<PublishLogsPage />)
  expect(await screen.findByText('第一次')).toBeInTheDocument()
  first.unmount()
  vi.mocked(api.publishListRecords).mockResolvedValue([record({ status: 'published', title: '再次打开' })])
  render(<PublishLogsPage />)
  expect(await screen.findByText('再次打开')).toBeInTheDocument()
  expect(vi.mocked(api.publishListRecords).mock.calls.length).toBeGreaterThanOrEqual(2)
})

it('shows unsupported stats instead of zero and keeps a real zero', async () => {
  vi.mocked(api.publishListRecords).mockResolvedValue([record({ status: 'published', title: '已上线' })])
  const stats: VideoStats = {
    recordId: 'rec-1', values: { views: 0, likes: 4 }, unsupported: ['shares'], fetchedAt: '2026-01-01T00:00:00Z', error: null,
  }
  vi.mocked(api.publishStats).mockResolvedValue(stats)
  render(<PublishDataPage />)
  expect(await screen.findByText('已上线')).toBeInTheDocument()
  expect(await screen.findByText('0')).toBeInTheDocument()
  expect(screen.getByText('4')).toBeInTheDocument()
  expect(screen.getAllByText('平台不支持').length).toBeGreaterThanOrEqual(2)
})

it('pastes a TikTok callback into complete', async () => {
  vi.mocked(api.publishBegin).mockResolvedValue({ requestId: 'req-1', url: 'https://www.tiktok.com/v2/auth/authorize/?x=1', mode: 'redirect' })
  vi.mocked(api.publishComplete).mockResolvedValue(account)
  render(<AccountsPage />)
  fireEvent.click(screen.getByRole('button', { name: '绑定账号' }))
  fireEvent.change(await screen.findByLabelText('Client ID'), { target: { value: 'client' } })
  fireEvent.change(screen.getByLabelText('Client Secret'), { target: { value: 'secret' } })
  fireEvent.change(screen.getByLabelText('回调地址'), { target: { value: 'https://example.com/cb' } })
  fireEvent.click(screen.getByRole('button', { name: '开始授权' }))
  fireEvent.change(await screen.findByLabelText('授权回调'), { target: { value: 'https://example.com/cb?code=abc&state=1' } })
  fireEvent.click(screen.getByRole('button', { name: '完成绑定' }))
  await waitFor(() => expect(api.publishComplete).toHaveBeenCalledWith('req-1', 'https://example.com/cb?code=abc&state=1'))
  expect(api.publishBegin).toHaveBeenCalledWith({ platform: 'tiktok', clientId: 'client', clientSecret: 'secret', redirectUri: 'https://example.com/cb' })
})
