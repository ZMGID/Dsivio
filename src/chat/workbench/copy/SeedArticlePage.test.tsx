import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { StrictMode } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import { i18n } from '../../../components/i18n'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { SeedArticlePage } from './SeedArticlePage'

vi.mock('../../../api/tauri', () => ({
  api: {
    listMediaTasks: vi.fn(),
    recordMediaOutput: vi.fn(),
    runAiTask: vi.fn(),
    cancelAiTask: vi.fn(),
    openLocalFile: vi.fn(),
    exportMediaOutput: vi.fn(),
  },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn(), open: vi.fn() }))

const t = i18n.zh

function article(partial: Partial<MediaTask> = {}): MediaTask {
  return {
    id: 'article-1',
    providerId: 'local',
    model: 'record',
    kind: 'text',
    status: 'succeeded',
    createdAt: '2026-10-02T00:00:00Z',
    error: null,
    remoteId: null,
    outputs: [{ path: '/tmp/articles/output.md', mime: 'text/markdown' }],
    canResume: false,
    origin: 'workbench/articles',
    prompt: '通勤帆布包',
    result: { title: '通勤包推荐' },
    requestHash: null,
    cancellation: null,
    ...partial,
  }
}

const aiResult = { text: '# 通勤包\n容量大，适合每天挤地铁。', toolCalls: [], usage: null }

beforeEach(() => {
  vi.mocked(api.listMediaTasks).mockReset()
  vi.mocked(api.recordMediaOutput).mockReset()
  vi.mocked(api.runAiTask).mockReset()
  vi.mocked(api.cancelAiTask).mockReset()
  vi.mocked(api.openLocalFile).mockReset()
  vi.mocked(api.exportMediaOutput).mockReset()
  vi.mocked(save).mockReset()
  vi.mocked(api.listMediaTasks).mockResolvedValue([])
  vi.mocked(api.recordMediaOutput).mockResolvedValue(article())
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, status: 200, text: async () => '# 通勤包推荐\n上次写的正文' })))
})

async function typeBrief(value: string) {
  fireEvent.change(screen.getByPlaceholderText(t.workbenchArticlesBriefHint), { target: { value } })
}

async function editorWith(snippet: string) {
  let match: HTMLTextAreaElement | undefined
  await waitFor(() => {
    match = (screen.getAllByRole('textbox') as HTMLTextAreaElement[]).find((box) => box.value.includes(snippet))
    expect(match).toBeTruthy()
  })
  return match!
}

it('submits one assembled request and records the article', async () => {
  vi.mocked(api.runAiTask).mockResolvedValue(aiResult)
  render(<SeedArticlePage />)
  await typeBrief('通勤帆布包，容量大')
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  await waitFor(() => expect(api.runAiTask).toHaveBeenCalledTimes(1))
  const request = vi.mocked(api.runAiTask).mock.calls[0][0]
  expect(request.mode).toBe('once')
  expect(request.prompt).toContain('通勤帆布包，容量大')
  expect(request.prompt).toContain('发布平台：公众号推文（wechat）')
  expect(request.prompt).toContain('文章类型：种草推荐（seed）')
  expect(request.prompt).toContain('篇幅：800-1100字')
  expect(request.images).toEqual([])
  await waitFor(() => expect(api.recordMediaOutput).toHaveBeenCalledWith({
    origin: 'workbench/articles',
    title: '通勤包',
    text: aiResult.text,
    prompt: '通勤帆布包，容量大',
  }))
  expect((await editorWith('适合每天挤地铁')).value).toBe(aiResult.text)
})

it('shows a failed write, then retries into a saved article', async () => {
  vi.mocked(api.runAiTask).mockRejectedValueOnce(new Error('no key'))
  render(<SeedArticlePage />)
  await typeBrief('通勤帆布包')
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  expect((await screen.findAllByText('no key')).length).toBeGreaterThan(0)
  expect(api.recordMediaOutput).not.toHaveBeenCalled()
  vi.mocked(api.runAiTask).mockResolvedValueOnce(aiResult)
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  await waitFor(() => expect(api.recordMediaOutput).toHaveBeenCalledTimes(1))
  expect((await editorWith('适合每天挤地铁')).value).toBe(aiResult.text)
})

it('does not record a result that arrives after leaving the page', async () => {
  let resolveRun: (value: typeof aiResult) => void = () => {}
  vi.mocked(api.runAiTask).mockImplementation(() => new Promise((resolve) => { resolveRun = resolve }))
  const view = render(<SeedArticlePage />)
  await typeBrief('通勤帆布包')
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  await waitFor(() => expect(api.runAiTask).toHaveBeenCalledTimes(1))
  view.unmount()
  await act(async () => {
    resolveRun(aiResult)
    await Promise.resolve()
  })
  expect(api.recordMediaOutput).not.toHaveBeenCalled()
})

it('keeps the edited article visible when saving fails', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([article()])
  render(<SeedArticlePage />)
  fireEvent.click(await screen.findByRole('button', { name: '通勤包推荐' }))
  const editor = await editorWith('上次写的正文')
  fireEvent.change(editor, { target: { value: '# 通勤包推荐\n改过的正文' } })
  vi.mocked(api.recordMediaOutput).mockRejectedValueOnce(new Error('磁盘满了'))
  fireEvent.click(screen.getByRole('button', { name: '保存文章' }))
  expect((await screen.findAllByText('磁盘满了')).length).toBeGreaterThan(0)
  expect((await editorWith('改过的正文')).value).toContain('改过的正文')
})

it('opens and exports the saved document, and shows an export failure', async () => {
  vi.mocked(api.listMediaTasks).mockResolvedValue([article()])
  vi.mocked(save).mockResolvedValue('/tmp/exported.md')
  render(<SeedArticlePage />)
  fireEvent.click(await screen.findByRole('button', { name: '通勤包推荐' }))
  await editorWith('上次写的正文')
  fireEvent.click(screen.getByRole('button', { name: '打开文档' }))
  await waitFor(() => expect(api.openLocalFile).toHaveBeenCalledWith('/tmp/articles/output.md'))
  expect(api.recordMediaOutput).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: '导出文档' }))
  await waitFor(() => expect(api.exportMediaOutput).toHaveBeenCalledWith('article-1', '/tmp/exported.md'))
  vi.mocked(api.exportMediaOutput).mockRejectedValueOnce(new Error('没有权限'))
  fireEvent.click(screen.getByRole('button', { name: '导出文档' }))
  expect((await screen.findAllByText('没有权限')).length).toBeGreaterThan(0)
})

it('shows a prior article again after leaving and reopening, ignoring a late list', async () => {
  let resolveFirst: (value: MediaTask[]) => void = () => {}
  vi.mocked(api.listMediaTasks).mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve }))
  const view = render(<SeedArticlePage />)
  await waitFor(() => expect(api.listMediaTasks).toHaveBeenCalledTimes(1))
  expect(api.listMediaTasks).toHaveBeenCalledWith({ origin: 'workbench/articles' })
  vi.mocked(api.listMediaTasks).mockResolvedValueOnce([article({ id: 'new', result: { title: '新文章' } })])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('button', { name: '新文章' })).toBeInTheDocument()
  await act(async () => { resolveFirst([article({ id: 'old', result: { title: '旧文章' } })]) })
  expect(screen.queryByRole('button', { name: '旧文章' })).toBeNull()
  view.unmount()
  vi.mocked(api.listMediaTasks).mockResolvedValue([article()])
  render(<SeedArticlePage />)
  expect(await screen.findByRole('button', { name: '通勤包推荐' })).toBeInTheDocument()
})

it('does not call the model when product info and images are both missing', async () => {
  render(<SeedArticlePage />)
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  expect((await screen.findAllByText(t.workbenchArticlesNeedBrief)).length).toBeGreaterThan(0)
  expect(api.runAiTask).not.toHaveBeenCalled()
})

it('shows the validation error after an expired history-load error', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  try {
    vi.mocked(api.listMediaTasks).mockRejectedValue(new Error('历史读取失败'))
    render(<SeedArticlePage />)
    expect(await screen.findByRole('alert')).toHaveTextContent('历史读取失败')
    act(() => { vi.advanceTimersByTime(5000) })
    expect(screen.queryByRole('alert')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
    expect(await screen.findByRole('alert')).toHaveTextContent(t.workbenchArticlesNeedBrief)
  } finally {
    vi.useRealTimers()
  }
})

it('still generates and records after React StrictMode remounts the page', async () => {
  vi.mocked(api.runAiTask).mockResolvedValue(aiResult)
  render(<StrictMode><SeedArticlePage /></StrictMode>)
  await typeBrief('通勤帆布包，容量大')
  fireEvent.click(screen.getByRole('button', { name: '生成种草文章' }))
  expect((await editorWith('适合每天挤地铁')).value).toBe(aiResult.text)
  await waitFor(() => expect(api.recordMediaOutput).toHaveBeenCalledTimes(1))
})
