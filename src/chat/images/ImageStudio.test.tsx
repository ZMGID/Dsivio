import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { api } from '../../api/tauri'
import ImageStudio from './ImageStudio'
import { open } from '@tauri-apps/plugin-dialog'
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
    studioTaskLibrary: vi.fn(async () => ({})),
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

async function openSavedTask(name: string) {
  fireEvent.click(await screen.findByRole('button', { name: /^任务 / }))
  fireEvent.click(await screen.findByRole('button', { name: `打开任务 ${name}` }))
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
  it.each([false, true])('saves edited plans before switching tasks (failure=%s)', async (fails) => {
    const current = fixture()
    const target = { ...fixture(), id: 'second', brief: { ...fixture().brief, name: '另一个任务' } }
    const plans = current.plans.map(p => ({ ...p, prompt: '尚未保存的方案修改' }))
    localStorage.setItem('dsivio-image-draft-v1', JSON.stringify({
      brief: current.brief, taskId: current.id, revision: current.revision, plans,
    }))
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([current, target]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(target)
    if (fails) vi.mocked(api.imageStudioSavePlans).mockRejectedValueOnce(new Error('保存方案失败'))
    else vi.mocked(api.imageStudioSavePlans).mockResolvedValueOnce({ ...current, plans })
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: /^任务 / }))
    await waitFor(() => expect(api.imageStudioSavePlans).toHaveBeenCalledWith(current.id, current.revision, plans))
    if (fails) {
      expect(await screen.findByText('保存方案失败')).toBeTruthy()
      expect(JSON.parse(localStorage.getItem('dsivio-image-draft-v1')!).plans).toEqual(plans)
      expect(api.imageStudioGet).not.toHaveBeenCalled()
    } else {
      fireEvent.click(await screen.findByRole('button', { name: '打开任务 另一个任务' }))
      await waitFor(() => expect(api.imageStudioGet).toHaveBeenCalledWith(target.id))
      expect(api.imageStudioSave).not.toHaveBeenCalled()
    }
  })
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
  it('starts a text-only image directly, with no preparation controls or mandatory plan step', async () => {
    const saved = fixture()
    saved.brief = { ...emptyBrief('gen'), requirement: '一只白色马克杯，不要文字' }
    vi.mocked(api.imageStudioSave).mockResolvedValue(saved)
    vi.mocked(api.imageStudioAction).mockResolvedValue({ ...saved, status: 'running' })
    render(<ImageStudio />)
    expect(await screen.findByRole('button', { name: '生成图片' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '比例' })).toHaveTextContent('1:1 正方形')
    expect(screen.getByRole('button', { name: '分辨率' })).toHaveTextContent('1K · 1024×1024')
    expect(screen.queryByRole('button', { name: '画幅' })).not.toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: saved.brief.requirement } })
    fireEvent.click(screen.getByRole('button', { name: '生成图片' }))
    await waitFor(() => expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, { kind: 'start', group: '未分类' }))
    expect(api.imageStudioSave).toHaveBeenCalledWith(expect.objectContaining({ language: '无文字', products: [] }), undefined, undefined)
    expect(screen.queryByRole('tab', { name: /画面方案/ })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('商品正面')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('商品背面')).not.toBeInTheDocument()
  })

  it('keeps replacement examples separate from product photos and accepts no extra prompt', async () => {
    const sample = { id: 'layout', path: 'assets/layout.png', name: '样图.png' }
    vi.mocked(open).mockResolvedValue(['C:/layout.png'])
    vi.mocked(api.imageStudioImport).mockResolvedValueOnce([{ ...importedProduct(), assets: [sample] }])
    vi.mocked(api.imageStudioSave).mockImplementation(async (brief) => ({ ...fixture(), brief }))
    vi.mocked(api.imageStudioAction).mockResolvedValue({ ...fixture(), status: 'running' })
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '样图换货' }))
    expect(screen.queryByLabelText('自定义图内语言')).not.toBeInTheDocument()
    expect(screen.getByText('跟随样图')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '开始换货' })).toBeDisabled()
    expect(screen.getByText('已有换货模板')).toBeInTheDocument()
    expect(screen.getByLabelText('搜索模板')).toBeInTheDocument()
    expect(screen.queryByText('使用已有换货模板')).not.toBeInTheDocument()
    expect(screen.getByLabelText('样图投放区')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '选择现成套图' }))
    await screen.findByRole('button', { name: '移除样图 1' })
    expect(screen.getByRole('button', { name: '开始换货' })).toBeDisabled()
    vi.mocked(api.imageStudioImport).mockResolvedValue([importedProduct()])
    dropHandler?.({ payload: { type: 'drop', paths: ['C:/product.png'] } })
    await waitFor(() => expect(screen.getByRole('button', { name: '开始换货' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: '开始换货' }))
    await waitFor(() =>     expect(api.imageStudioSave).toHaveBeenCalledWith(expect.objectContaining({
      feature: 'replace', requirement: '', language: '跟随样图', templateId: null,
      products: [importedProduct()], workflowInput: { mode: 'replace', sources: [sample] },
    }), undefined, undefined))
  })

  it('searches templates beside the product dropzone and keeps matching heights', async () => {
    const data = bootstrap()
    data.templates = [
      {
        id: 'tpl-default',
        directory: '',
        builtin: true,
        data: {
          name: '默认电商套图',
          mode: 'smart',
          slots: [
            { id: 'h1', purpose: '主图 · 展示' },
            { id: 'h2', purpose: '核心卖点 · 三点' },
          ],
        },
      },
      {
        id: 'tpl-kids',
        directory: '',
        builtin: true,
        data: { name: '童装套图', mode: 'smart', slots: [{ id: 'k1', purpose: '爆款首图' }] },
      },
    ]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(data)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '模板套图' }))
    const search = await screen.findByLabelText('搜索模板')
    expect(screen.getByRole('option', { name: /默认电商套图/ })).toBeInTheDocument()
    fireEvent.change(search, { target: { value: '童装' } })
    expect(screen.queryByRole('option', { name: /默认电商套图/ })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('option', { name: /童装套图/ }))
    expect(screen.getByRole('option', { name: /童装套图/ })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByLabelText('商品素材投放区')).toBeInTheDocument()
  })

  it('reports a local draft failure instead of claiming it was saved', async () => {
    const storage = vi.spyOn(localStorage, 'setItem').mockImplementation(() => { throw new Error('quota exceeded') })
    try {
      render(<ImageStudio />)
      const toast = await screen.findByRole('alert')
      expect(toast).toHaveTextContent('本机草稿未能保存，请先保存任务。')
      expect(toast.className).toContain('studio-toast')
      expect(toast.closest('.is-main')).toBeNull()
      expect(screen.getByRole('button', { name: '保存任务' })).toBeEnabled()
      expect(screen.queryByText('草稿保存在本机')).not.toBeInTheDocument()
    } finally { storage.mockRestore() }
  })

  it('refreshes shared chat templates on focus without replacing the unsaved image brief', async () => {
    render(<ImageStudio />)
    await waitFor(() => expect(screen.getByRole('button', { name: '生成图片' })).toBeDisabled())
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
    await waitFor(() => expect(screen.getByRole('button', { name: '生成图片' })).toBeDisabled())
    expect(screen.getByRole('region', { name: '图片工作台' })).toHaveClass('kv')
    expect(screen.getByRole('main')).toHaveClass('custom-scrollbar')
    expect(screen.getByLabelText('图片要求')).toHaveClass('kv-textarea', 'custom-scrollbar')
    expect(screen.queryByRole('button', { name: '选择助手' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByText('更多设置'))
    expect(screen.queryByText('更多场景')).not.toBeInTheDocument()
    expect(screen.queryByText('最近任务')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: '任务 0' })).toBeInTheDocument()
    const platform = screen.getByRole('button', { name: '使用平台' })
    expect(platform).toHaveClass('kv-select')
    fireEvent.click(platform)
    expect(await screen.findByRole('listbox')).toHaveClass('kv-select-menu', 'custom-scrollbar')
    fireEvent.click(screen.getByRole('option', { name: 'Amazon' }))
    expect(screen.getByRole('button', { name: '使用平台' })).toHaveTextContent('Amazon')
    expect(screen.queryByLabelText('任务名称')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '图片设置' }))
    expect(screen.getByRole('dialog', { name: '图片设置' })).toHaveClass('kv-modal', 'custom-scrollbar')
  })
  it('opens saved tasks from a dedicated page instead of the rail', async () => {
    const t = fixture()
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '任务 1' }))
    expect(screen.getByRole('heading', { name: /^图片任务/ })).toBeInTheDocument()
    expect(screen.queryByText('最近任务')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: '打开任务 背包秋季套图' }))
    await waitFor(() => expect(api.imageStudioGet).toHaveBeenCalledWith('job'))
    expect(screen.getByRole('heading', { name: '模板套图' })).toBeInTheDocument()
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
    await waitFor(() => expect(screen.getByRole('button', { name: '生成图片' })).toBeDisabled())
    fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: '纯白底，保留产品细节' } })
    fireEvent.click(screen.getByRole('button', { name: '生成图片' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, { kind: 'start', group: '未分类' }),
    )
    expect(api.imageStudioSave).toHaveBeenCalledWith(
      expect.objectContaining({ feature: 'gen', requirement: '纯白底，保留产品细节', ratio: '1:1' }),
      undefined,
      undefined,
    )
    expect(screen.getByRole('tab', { name: /生成结果/ })).toHaveAttribute('aria-selected', 'true')
  })
  it('does not offer bulk approval before all sample pages have succeeded', async () => {
    const t = fixture()
    t.results = [result('a')]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    render(<ImageStudio />)
    await openSavedTask('背包秋季套图')
    expect(await screen.findByRole('button', { name: /确认样品效果/ })).toBeDisabled()
    expect(api.imageStudioAction).not.toHaveBeenCalled()
  })
  it('shows results for the selected category and hides bulk controls when none remain', async () => {
    const t = fixture()
    t.brief.feature = 'client'
    t.brief.products[2].category = '童装'
    t.results = [result('a'), result('b'), result('c')]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    render(<ImageStudio />)
    await openSavedTask('背包秋季套图')
    expect(await screen.findAllByRole('button', { name: '查看 / 修改' })).toHaveLength(2)
    expect(screen.queryByRole('button', { name: '确认样品效果' })).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: /童装/ }))
    expect(screen.getAllByRole('button', { name: '查看 / 修改' })).toHaveLength(1)
    expect(screen.queryByRole('button', { name: '确认样品效果' })).not.toBeInTheDocument()
  })
  it('records human approval as a separate action, then offers bulk generation', async () => {
    const t = fixture()
    t.results = [result('a'), result('b')]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue({ ...t, approvedGroups: ['背包'] })
    render(<ImageStudio />)
    await openSavedTask('背包秋季套图')
    fireEvent.click(await screen.findByRole('button', { name: /确认样品效果/ }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, { kind: 'approve', group: '背包' }),
    )
    expect(api.imageStudioAction).toHaveBeenCalledTimes(1)
    expect(await screen.findByRole('button', { name: /生成剩余商品/ })).toBeEnabled()
  })
  it('resumes a persisted remote job instead of submitting a second generation', async () => {
    const t = fixture()
    t.results = [{ ...result('a', 2, null), remoteId: 'remote-123', error: '等待超时' }]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue(t)
    render(<ImageStudio />)
    await openSavedTask('背包秋季套图')
    fireEvent.click(await screen.findByRole('button', { name: '恢复查询' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('job', 2, {
        kind: 'resume',
        resultId: 'a-2',
        group: '背包',
      }),
    )
  })
  it('resumes a saved CDN download without submitting another generation', async () => {
    const t = fixture()
    t.results = [{ ...result('a', 2, null), downloadUrl: 'https://cdn.example/image.png', error: '等待超时' }]
    vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([t]))
    vi.mocked(api.imageStudioGet).mockResolvedValue(t)
    vi.mocked(api.imageStudioAction).mockResolvedValue(t)
    render(<ImageStudio />)
    await openSavedTask('背包秋季套图')
    fireEvent.click(await screen.findByRole('button', { name: '恢复下载' }))
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
    t.brief.products.push({ ...t.brief.products[0], id: 'd', name: 'd' })
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
    await openSavedTask('背包秋季套图')
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

describe('Replace sample drag-drop', () => {
  it('imports images dropped on the sample panel as layout sources', async () => {
    const sample = { id: 'layout', path: 'assets/layout.png', name: '样图.png' }
    vi.mocked(api.imageStudioImport).mockResolvedValue([{ ...importedProduct(), assets: [sample] }])
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '样图换货' }))
    const zone = await screen.findByLabelText('样图投放区')
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    fireEvent.dragEnter(zone)
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(zone).toHaveClass('is-drop-active'))
    expect(screen.getByLabelText('商品素材投放区')).not.toHaveClass('is-drop-active')
    dropHandler?.({ payload: { type: 'drop', paths: ['C:/layout.png'] } })
    await waitFor(() => expect(api.imageStudioImport).toHaveBeenCalledWith(['C:/layout.png'], false))
    expect(await screen.findByRole('button', { name: '移除样图 1' })).toBeInTheDocument()
    expect(screen.getByLabelText('商品素材投放区')).not.toHaveClass('is-upload-area--filled')
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
    fireEvent.click(screen.getByText('更多设置'))
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
    fireEvent.click(screen.getByText('更多设置'))
    expect(screen.getByLabelText('商品信息（可选）')).toBeInTheDocument()
  })
})

it('keeps image work alive through chat navigation and does not replace a new brief', async () => {
  const saved = { ...fixture(), brief: { ...emptyBrief('gen'), name: '后台图片', requirement: '原来的图片要求' } }
  let finish!: (task: ImageTask) => void
  const pending = new Promise<ImageTask>(resolve => { finish = resolve })
  vi.mocked(api.imageStudioSave).mockResolvedValue(saved)
  vi.mocked(api.imageStudioAction).mockReturnValue(pending)
  const { ChatRouteKeepAlive } = await import('../ChatRouteKeepAlive')
  const page = () => <ChatRouteKeepAlive activeKey="images"><ImageStudio /></ChatRouteKeepAlive>
  const { rerender } = render(page())
  await waitFor(() => expect(api.imageStudioBootstrap).toHaveBeenCalled())
  fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: saved.brief.requirement } })
  fireEvent.click(screen.getByRole('button', { name: '生成图片' }))
  await waitFor(() => expect(api.imageStudioAction).toHaveBeenCalled())
  fireEvent.click(screen.getByRole('button', { name: /任务 \d/ }))
  expect(screen.getByRole('heading', { name: /图片任务/ })).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '新建图片' }))
  fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: '新的图片要求' } })
  rerender(<ChatRouteKeepAlive activeKey="conversation"><main>新聊天</main></ChatRouteKeepAlive>)
  const completed = { ...saved, status: 'completed', progress: '已完成' }
  await act(async () => { finish(completed); await pending })
  rerender(page())
  expect(screen.getByLabelText('图片要求')).toHaveValue('新的图片要求')
  expect(api.imageStudioAction).toHaveBeenCalledTimes(1)
})


it('keeps unsaved changes when an existing task changes during a focus refresh', async () => {
  const task = fixture()
  task.brief = { ...emptyBrief('gen'), requirement: 'saved request' }
  localStorage.setItem('dsivio-image-draft-v1', JSON.stringify({ taskId: task.id, revision: task.revision, brief: task.brief }))
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([task]))
  render(<ImageStudio />)
  await waitFor(() => expect(screen.getByLabelText('图片要求')).toHaveValue('saved request'))
  fireEvent.change(screen.getByLabelText('图片要求'), { target: { value: 'local edit' } })
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([{ ...task, revision: task.revision + 1, progress: 'remote change' }]))
  await act(async () => { fireEvent.focus(window) })
  expect(screen.getByLabelText('图片要求')).toHaveValue('local edit')
})


it('keeps image task controls stable during reads and ignores abandoned destinations', async () => {
  const target = fixture()
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue(bootstrap([target]))
  let finish!: (value: ImageTask) => void
  vi.mocked(api.imageStudioGet).mockReturnValueOnce(new Promise(resolve => { finish = resolve }))
  render(<ImageStudio />)
  await openSavedTask(target.brief.name)
  expect(screen.getByRole('button', { name: '刷新任务' })).toBeEnabled()
  expect(screen.getByRole('button', { name: '新建图片' })).toBeEnabled()
  expect(screen.getByRole('button', { name: '图片设置' })).toBeEnabled()
  fireEvent.click(screen.getByRole('button', { name: /模板库/ }))
  await act(async () => finish(target))
  expect(screen.getByRole('button', { name: /模板库/ })).toHaveAttribute('aria-current', 'page')
})
