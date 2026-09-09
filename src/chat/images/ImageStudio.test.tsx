import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { api } from '../../api/tauri'
import ImageStudio from './ImageStudio'
import {
  emptyBrief,
  latestResults,
  sampleComplete,
  type ImageBootstrap,
  type ImageProduct,
  type ImageResult,
  type ImageTask,
} from './types'

let dropHandler:
  | ((event: { payload: { type: string; paths?: string[] } }) => void)
  | undefined

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    imageStudioBootstrap: vi.fn(),
    imageStudioGet: vi.fn(),
    imageStudioSave: vi.fn(),
    imageStudioAction: vi.fn(),
    imageStudioPreview: vi.fn(),
    imageStudioSavePlans: vi.fn(),
    imageStudioImport: vi.fn(),
    imageStudioConfig: vi.fn(),
  },
}))
vi.mock('../../api/settingsCache', () => ({
  getSettingsCached: vi.fn().mockResolvedValue({ providers: [] }),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../api', () => ({
  chatApi: {
    getAssistants: vi.fn().mockResolvedValue([]),
    optimizePrompt: vi.fn(),
  },
}))
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

const config = {
  providerId: 'p',
  model: 'image-test',
  protocol: 'openai',
  agentProviderId: '',
  agentModel: '',
}
function fixture(): ImageTask {
  return {
    id: 'job',
    revision: 2,
    createdAt: '',
    updatedAt: '',
    brief: {
      ...emptyBrief('smart'),
      name: '背包秋季套图',
      templateId: 'tpl',
      products: ['a', 'b', 'c'].map((id) => ({
        id,
        name: id,
        category: '背包',
        kind: '',
        facts: '',
        assets: [],
        front: null,
        back: null,
        templateId: null,
      })),
    },
    plans: ['a', 'b'].map((id) => ({
      productId: id,
      slotId: 'h1',
      purpose: '主视觉',
      prompt: 'product',
      copy: '',
      refs: [],
    })),
    results: [],
    approvedGroups: [],
    status: 'ready',
    progress: '方案已生成',
    error: null,
    templates: [],
  }
}
function result(productId: string, revision = 2, path: string | null = 'results/a.png'): ImageResult {
  return {
    id: `${productId}-${revision}`,
    productId,
    slotId: 'h1',
    revision,
    path,
    error: null,
    remoteId: null,
    prompt: 'test prompt',
    width: 800,
    height: 800,
    review: null,
    config,
  }
}
function bootstrap(tasks: ImageTask[] = []): ImageBootstrap {
  return {
    tasks,
    config,
    providers: [{ id: 'p', name: 'Test provider', models: ['image-test'], ready: true }],
    templates: [],
  }
}
function importedProduct(name = '商品素材'): ImageProduct {
  return {
    id: 'imported',
    name,
    category: '未分类',
    kind: '',
    facts: '',
    front: 'asset-1',
    back: null,
    assets: [{ id: 'asset-1', name: 'front.png', path: 'imports/front.png' }],
    templateId: null,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  dropHandler = undefined
  localStorage.clear()
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap())
  vi.mocked(api.imageStudioPreview).mockResolvedValue('data:image/png;base64,iVBORw0KGgo=')
  vi.mocked(api.imageStudioImport).mockResolvedValue([importedProduct()])
})

describe('Image workspace state gates', () => {
  it('requires both complete samples, and rejects stale images or a failed latest edit', () => {
    const t = fixture()
    t.results = [result('a'), result('b', 1)]
    expect(sampleComplete(t, '背包')).toBe(false)
    t.results.push(result('b'))
    expect(sampleComplete(t, '背包')).toBe(true)
    t.results.push({ ...result('a', 2, null), id: 'edit-failed' })
    expect(sampleComplete(t, '背包')).toBe(false)
    expect(latestResults(t)).toHaveLength(2)
    expect(latestResults(t)[0].path).toBeNull()
  })
  it('keeps other categories behind their own sample gate', () => {
    const t = fixture()
    t.results = [result('a'), result('b')]
    t.brief.products[2].category = '童装'
    expect(sampleComplete(t, '背包')).toBe(true)
    expect(sampleComplete(t, '童装')).toBe(false)
  })
})

describe('Built-in image workflows', () => {
  it('refreshes shared chat templates on focus without replacing the unsaved image brief', async () => {
    render(<ImageStudio />)
    await waitFor(() => expect(screen.getByRole('button', { name: '生成画面方案' })).toBeEnabled())
    fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: '尚未保存的需求' } })
    const next = bootstrap()
    next.templates = [
      {
        id: 'chat-template',
        directory: 'templates/chat-template',
        builtin: false,
        data: { name: '聊天新建模板', mode: 'smart', slots: [{ id: 'h1', brief: '场景主图' }] },
      },
    ]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(next)
    fireEvent(window, new Event('focus'))
    await waitFor(() => expect(screen.getByRole('button', { name: '模板库 1' })).toBeInTheDocument())
    expect(screen.getByLabelText('图片要求')).toHaveValue('尚未保存的需求')
    const saved = fixture()
    saved.brief = { ...emptyBrief('gen'), requirement: '尚未保存的需求' }
    vi.mocked(api.imageStudioSave).mockResolvedValue(saved)
    fireEvent.click(screen.getByRole('button', { name: '模板库 1' }))
    expect(await screen.findByRole('heading', { name: '聊天新建模板' })).toBeInTheDocument()
  })
  it('uses the shared theme, scrollbar, input and select controls', async () => {
    render(<ImageStudio />)
    await waitFor(() => expect(screen.getByRole('button', { name: '生成画面方案' })).toBeEnabled())
    expect(screen.getByRole('region', { name: '图片工作台' })).toHaveClass('kv')
    expect(screen.getByRole('main')).toHaveClass('custom-scrollbar')
    expect(screen.getByLabelText('图片要求')).toHaveClass('kv-textarea', 'custom-scrollbar')
    expect(screen.getByRole('button', { name: '先写下图片要求或加载素材' })).toBeDisabled()
    expect(screen.queryByText('更多场景')).not.toBeInTheDocument()
    const platform = screen.getByRole('button', { name: '使用平台' })
    expect(platform).toHaveClass('kv-select')
    fireEvent.click(platform)
    expect(await screen.findByRole('listbox')).toHaveClass('kv-select-menu', 'custom-scrollbar')
    fireEvent.click(screen.getByRole('option', { name: 'Amazon' }))
    expect(screen.getByRole('button', { name: '使用平台' })).toHaveTextContent('Amazon')
    fireEvent.click(screen.getByRole('button', { name: '图片设置' }))
    expect(screen.getByRole('dialog', { name: '图片设置' })).toHaveClass('kv-modal', 'custom-scrollbar')
  })
  it('restores unsaved edits without replacing a newer task revision', async () => {
    const t = fixture()
    localStorage.setItem(
      'dsivio-image-draft-v1',
      JSON.stringify({ taskId: t.id, revision: 1, brief: { ...t.brief, requirement: 'outdated draft' } }),
    )
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    render(<ImageStudio />)
    await waitFor(() => expect(screen.getByLabelText('图片要求')).toHaveValue(t.brief.requirement))
  })
  it('saves structured inputs before invoking a dedicated image Agent action', async () => {
    const t = fixture()
    t.brief = { ...emptyBrief('gen'), name: '快速出图', requirement: '纯白底，保留产品细节' }
    vi.mocked(api.imageStudioSave).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue({ ...t, progress: '规划完成' })
    render(<ImageStudio />)
    await waitFor(() => expect(screen.getByRole('button', { name: '生成画面方案' })).toBeEnabled())
    fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: '纯白底，保留产品细节' } })
    fireEvent.click(screen.getByRole('button', { name: '生成画面方案' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, { kind: 'plan', group: '未分类' }),
    )
    expect(api.imageStudioSave).toHaveBeenCalledWith(
      expect.objectContaining({ feature: 'gen', requirement: '纯白底，保留产品细节', ratio: '1:1' }),
      undefined,
      undefined,
    )
    expect(screen.getByRole('tab', { name: /画面方案/ })).toHaveAttribute('aria-selected', 'true')
  })
  it('does not offer bulk approval before all sample pages have succeeded', async () => {
    const t = fixture()
    t.results = [result('a')]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '背包秋季套图' }))
    expect(await screen.findByRole('button', { name: /样品通过，允许批量/ })).toBeDisabled()
    expect(api.imageStudioAction).not.toHaveBeenCalled()
  })
  it('records human approval as a separate action, then offers bulk generation', async () => {
    const t = fixture()
    t.results = [result('a'), result('b')]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue({ ...t, approvedGroups: ['背包'] })
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '背包秋季套图' }))
    fireEvent.click(await screen.findByRole('button', { name: /样品通过，允许批量/ }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, { kind: 'approve', group: '背包' }),
    )
    expect(api.imageStudioAction).toHaveBeenCalledTimes(1)
    expect(await screen.findByRole('button', { name: /规划剩余商品并批量出图/ })).toBeEnabled()
  })
  it('resumes a persisted remote job instead of submitting a second generation', async () => {
    const t = fixture()
    t.results = [{ ...result('a', 2, null), remoteId: 'remote-123', error: '等待超时' }]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue(t)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '背包秋季套图' }))
    fireEvent.click(await screen.findByRole('button', { name: '恢复查询' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, {
        kind: 'resume',
        resultId: 'a-2',
        group: '背包',
      }),
    )
  })
  it('shows concurrent pending pages alongside completed and failed results', async () => {
    const t = fixture()
    t.status = 'running'
    t.progress = '并发生成中 · 已完成 2/4 张 · 进行中 2 张 · 失败 1 张'
    t.results = [
      result('a'),
      result('b', 2, null),
      { ...result('c', 2, null), remoteId: 'remote-123' },
      { ...result('d', 2, null), error: '图片接口 HTTP 429' },
    ]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '背包秋季套图' }))
    const pending = await screen.findAllByText('正在生成')
    expect(pending).toHaveLength(2)
    for (const label of pending) {
      const card = label.closest('article')
      expect(card).not.toHaveClass('failed')
      expect(card?.querySelector('.is-spinning')).toBeInTheDocument()
    }
    expect(screen.getByText('图片接口 HTTP 429').closest('article')).toHaveClass('failed')
    expect(screen.getByRole('button', { name: '查看 / 修改' })).toBeEnabled()
    expect(screen.getByRole('button', { name: '恢复查询' })).toBeDisabled()
  })
})

describe('Product material drag-drop', () => {
  it('imports dropped images as reference photos', async () => {
    render(<ImageStudio />)
    const zone = await screen.findByLabelText('商品素材投放区')
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(zone).toHaveClass('is-drop-active'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\goods\\front.png'] } })
    await waitFor(() =>
      expect(api.imageStudioImport).toHaveBeenCalledWith(['C:\\goods\\front.png'], false),
    )
    expect(await screen.findByLabelText('商品信息（可选）')).toBeInTheDocument()
    expect(screen.queryByText('再加图')).not.toBeInTheDocument()
    expect(screen.queryByText('1 款')).not.toBeInTheDocument()
    expect(screen.getByLabelText('商品素材投放区')).toHaveClass('is-upload-area--filled')
    expect(screen.queryByLabelText('商品正面')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('商品名称 / SKU')).not.toBeInTheDocument()
  })
  it('imports dropped folders as SKUs', async () => {
    vi.mocked(api.imageStudioImport).mockResolvedValue([importedProduct('sku-a')])
    render(<ImageStudio />)
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({
      payload: { type: 'drop', paths: ['C:\\goods\\sku-a', 'C:\\goods\\back.jpg'] },
    })
    await waitFor(() =>
      expect(api.imageStudioImport).toHaveBeenCalledWith(
        ['C:\\goods\\sku-a', 'C:\\goods\\back.jpg'],
        true,
      ),
    )
    expect(await screen.findByText('sku-a')).toBeInTheDocument()
    expect(screen.getByLabelText('商品信息（可选）')).toBeInTheDocument()
  })
})
