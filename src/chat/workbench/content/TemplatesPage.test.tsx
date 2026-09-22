import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import type { ImageTemplate } from '../../../api/workbenchImageContracts'
import type { VideoTemplate } from '../../../generated/contentTemplates'
import { ImageTemplatesPage } from './ImageTemplatesPage'
import { VideoTemplatesPage } from './VideoTemplatesPage'
vi.mock('../../../api/tauri', () => ({ api: {
  imageTemplatesList: vi.fn(), imageTemplateImport: vi.fn(), imageTemplateSave: vi.fn(), imageTemplateExport: vi.fn(), imageTemplatePreview: vi.fn(),
  videoTemplatesList: vi.fn(), videoTemplateImport: vi.fn(), videoTemplateSave: vi.fn(), videoTemplateExport: vi.fn(),
} }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(() => new Promise(() => {})), subscribeSettings: vi.fn(() => () => {}) }))
const image: ImageTemplate = { id: 'image', directory: 'templates/image', builtin: false, data: { name: '商品套图', mode: 'smart', slots: [{ id: 'h1', brief: '规则', refs: ['@product.front'] }], custom: { keep: true } } }
const video: VideoTemplate = { id: 'video', name: '参考视频', kind: 'reference', script: '旧剧本', shots: [{ time: '0-2', purpose: '特写', action: '商品', camera: '推近', keep: true }], spec: { duration_seconds: 15, aspect_ratio: '9:16' }, data: { full_video_prompt: '旧剧本', reference_template: true, extra: '保留' } }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(api.imageTemplatesList).mockResolvedValue([structuredClone(image)])
  vi.mocked(api.videoTemplatesList).mockResolvedValue([structuredClone(video)])
})
it('edits image fields through the independent API and updates both list and detail from the saved result', async () => {
  vi.mocked(api.imageTemplateSave).mockImplementation(async value => ({ ...value, data: { ...value.data, name: '已保存名称' } }))
  render(<ImageTemplatesPage />)
  fireEvent.click(await screen.findByText('查看 / 编辑'))
  fireEvent.change(screen.getByLabelText('模板名称'), { target: { value: '编辑名称' } })
  fireEvent.click(screen.getByText('保存'))
  await screen.findByText('模板已保存')
  expect(api.imageTemplateSave).toHaveBeenCalledWith(expect.objectContaining({ data: { ...image.data, name: '编辑名称' } }))
  expect(screen.getByLabelText('模板名称')).toHaveValue('已保存名称')
  expect(screen.getAllByText('已保存名称')).toHaveLength(2)
})
it('keeps video provenance and raw JSON while editing script and shots', async () => {
  vi.mocked(api.videoTemplateSave).mockImplementation(async value => value)
  render(<VideoTemplatesPage />)
  fireEvent.click(await screen.findByText('查看 / 编辑'))
  fireEvent.change(screen.getByLabelText('剧本'), { target: { value: '新剧本' } })
  fireEvent.change(screen.getByLabelText('动作'), { target: { value: '旋转商品' } })
  fireEvent.click(screen.getByText('保存'))
  await screen.findByText('模板已保存')
  const saved = vi.mocked(api.videoTemplateSave).mock.calls[0][0]
  expect(saved.data).toEqual(video.data); expect(saved.kind).toBe('reference')
  expect(saved.shots[0]).toEqual({ ...video.shots[0], action: '旋转商品' })
  expect(saved.script).toBe('新剧本')
})
it('reports failures, blocks repeat save, retains the draft, and allows retry', async () => {
  let reject!: (e: Error) => void
  vi.mocked(api.videoTemplateSave).mockReturnValueOnce(new Promise((_, fail) => { reject = fail })).mockResolvedValue(video)
  render(<VideoTemplatesPage />)
  fireEvent.click(await screen.findByText('查看 / 编辑'))
  const button = screen.getByText('保存')
  fireEvent.click(button); fireEvent.click(button)
  expect(button).toBeDisabled(); expect(api.videoTemplateSave).toHaveBeenCalledTimes(1)
  await act(async () => { reject(new Error('写入失败')) })
  expect(screen.getByRole('alert')).toHaveTextContent('写入失败')
  expect(screen.queryByText('模板已保存')).not.toBeInTheDocument()
  expect(screen.getByLabelText('剧本')).toHaveValue('旧剧本')
  fireEvent.click(button)
  await screen.findByText('模板已保存')
})
it('imports and exports images using only template endpoints', async () => {
  vi.mocked(open).mockResolvedValueOnce('/test/input').mockResolvedValueOnce('/test/output')
  vi.mocked(api.imageTemplateImport).mockResolvedValue({ ...image, id: 'imported', data: { ...image.data, name: '导入图' } })
  vi.mocked(api.imageTemplateExport).mockResolvedValue('/test/output/template')
  render(<ImageTemplatesPage />)
  await screen.findByText('商品套图')
  fireEvent.click(screen.getByText('导入文件夹'))
  await screen.findByDisplayValue('导入图')
  fireEvent.click(within(screen.getByRole('dialog')).getByText('导出'))
  await screen.findByText('/test/output/template')
  expect(api.imageTemplateImport).toHaveBeenCalledWith('/test/input')
  expect(api.imageTemplateExport).toHaveBeenCalledWith('imported', '/test/output')
})
it('imports and exports video JSON and exposes list failures with a retry', async () => {
  vi.mocked(api.videoTemplatesList).mockRejectedValueOnce(new Error('无法读取目录')).mockResolvedValue([video])
  vi.mocked(open).mockResolvedValueOnce('/test/video.json').mockResolvedValueOnce('/test/output')
  vi.mocked(api.videoTemplateImport).mockResolvedValue({ ...video, id: 'imported', name: '导入视频' })
  vi.mocked(api.videoTemplateExport).mockResolvedValue('/test/output/video.json')
  render(<VideoTemplatesPage />)
  expect(await screen.findByRole('alert')).toHaveTextContent('无法读取目录')
  fireEvent.click(screen.getByText('刷新'))
  await screen.findByText('参考视频')
  fireEvent.click(screen.getByText('导入 JSON'))
  await screen.findByDisplayValue('导入视频')
  fireEvent.click(within(screen.getByRole('dialog')).getByText('导出'))
  await screen.findByText('/test/output/video.json')
  expect(api.videoTemplateExport).toHaveBeenCalledWith('imported', '/test/output')
})
it('ignores a list response after leaving the page', async () => {
  let resolve!: (v: ImageTemplate[]) => void
  vi.mocked(api.imageTemplatesList).mockReturnValue(new Promise(done => { resolve = done }))
  const first = render(<ImageTemplatesPage />); first.unmount()
  render(<VideoTemplatesPage />)
  await screen.findByText('参考视频')
  await act(async () => { resolve([image]) })
  await waitFor(() => expect(screen.queryByText('商品套图')).not.toBeInTheDocument())
})

it('restores image cards with sample thumbnails, ordered page previews, and keyboard dismissal', async () => {
  vi.mocked(api.imageTemplatesList).mockResolvedValue([{ ...image, data: { ...image.data, mode: 'replace', slots: [
    { id: 'h1', purpose: '主视觉', example: 'front.png', prompt: '首页规则' },
    { id: 'h2', purpose: '背面', example: 'back.png', prompt: '背面规则' },
    { id: 'h3', purpose: '细节', example: 'detail.png', prompt: '细节规则' },
    { id: 'h4', purpose: '结尾', example: 'end.png', prompt: '结尾规则' },
  ] } }])
  vi.mocked(api.imageTemplatePreview).mockImplementation(async (_, ref) => `data:image/png;base64,${ref}`)
  const view = render(<ImageTemplatesPage />)
  expect(await screen.findByText('样图换货')).toBeVisible()
  expect(view.container.querySelectorAll('article.ct-image-card')).toHaveLength(1)
  expect(await screen.findByAltText('主视觉参考图')).toHaveAttribute('src', 'data:image/png;base64,front.png')
  expect(screen.getAllByRole('img')).toHaveLength(3)
  const trigger = screen.getByRole('button', { name: '商品套图 · 第 1 张：主视觉' })
  trigger.focus(); fireEvent.click(trigger)
  let dialog = screen.getByRole('dialog')
  expect(within(dialog).getByText('首页规则')).toBeVisible()
  expect(within(dialog).getByText('上一张')).toBeDisabled()
  fireEvent.keyDown(dialog, { key: 'Tab' })
  expect(within(dialog).getByRole('button', { name: '关闭页面预览' })).toHaveFocus()
  fireEvent.keyDown(document.activeElement!, { key: 'Tab', shiftKey: true })
  expect(within(dialog).getByText('下一张')).toHaveFocus()
  fireEvent.click(within(dialog).getByText('下一张'))
  expect(within(dialog).getByText('背面规则')).toBeVisible()
  fireEvent.click(within(dialog).getByText('下一张'))
  fireEvent.click(within(dialog).getByText('下一张'))
  dialog = screen.getByRole('dialog')
  expect(within(dialog).getByText('结尾规则')).toBeVisible()
  expect(within(dialog).getByText('下一张')).toBeDisabled()
  fireEvent.keyDown(dialog, { key: 'Escape' })
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  expect(trigger).toHaveFocus()
  expect(view.container.inert).toBeFalsy()
  expect(document.body.style.overflow).not.toBe('hidden')
})

it('shows rule-template page outlines and moves editing into a dismissible modal', async () => {
  render(<ImageTemplatesPage />)
  expect(await screen.findByText('页面安排')).toBeVisible()
  expect(screen.getByText('风格规则')).toBeVisible()
  expect(screen.queryByText(/smart|replace/)).not.toBeInTheDocument()
  expect(screen.getByText('创建规则模板')).toBeVisible()
  fireEvent.click(screen.getByText('查看 / 编辑'))
  const dialog = screen.getByRole('dialog')
  expect(within(dialog).getByLabelText('模板名称')).toHaveValue('商品套图')
  fireEvent.mouseDown(dialog.parentElement!)
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
})

it('restores video cards with shot summaries and expandable full script', async () => {
  const view = render(<VideoTemplatesPage />)
  await screen.findByText('参考视频')
  expect(view.container.querySelectorAll('article.vs-template')).toHaveLength(1)
  expect(screen.getByText('特写')).toBeVisible()
  expect(screen.getByText('0-2')).toBeVisible()
  expect(screen.getByText('参考模板 · 未验证成片')).toBeVisible()
  const summary = screen.getByText('查看剧本')
  expect(summary.tagName).toBe('SUMMARY')
  fireEvent.click(summary)
  expect(summary.closest('details')).toHaveAttribute('open')
  expect(screen.getByText('旧剧本')).toBeVisible()
  fireEvent.click(screen.getByText('查看 / 编辑'))
  expect(within(screen.getByRole('dialog')).getByLabelText('剧本')).toHaveValue('旧剧本')
})
