import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../api/tauri'
import ImageStudio from './ImageStudio'
import {
  emptyBrief,
  workflowSamplesComplete,
  type ImageBrief,
  type ImageProduct,
  type ImageResult,
  type ImageTask,
  type ImageTemplate,
} from './types'

vi.mock('../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    imageStudioBootstrap: vi.fn(),
    imageStudioGet: vi.fn(),
    imageStudioSave: vi.fn(),
    imageStudioAction: vi.fn(),
    imageStudioPreview: vi.fn(),
    imageStudioImport: vi.fn(),
    imageStudioExport: vi.fn(),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
let dropHandler:
  | ((event: { payload: { type: string; paths?: string[] } }) => void)
  | undefined
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
vi.mock('../../api/settingsCache', () => ({
  getSettingsCached: vi.fn().mockResolvedValue({ providers: [] }),
}))
vi.mock('../api', () => ({
  chatApi: { getAssistants: vi.fn().mockResolvedValue([]), optimizePrompt: vi.fn() },
}))

const config = {
  providerId: 'image',
  model: 'image-model',
  protocol: 'openai',
  agentProviderId: 'vision',
  agentModel: 'vision-model',
}
const source = { id: 'source', path: 'assets/source.png', name: '原始商品.png' }
function product(id: string): ImageProduct {
  return {
    id,
    name: `商品 ${id}`,
    facts: '',
    kind: '',
    category: '背包',
    front: id,
    back: null,
    templateId: null,
    assets: [{ id, path: `assets/${id}.png`, name: `${id}.png` }],
  }
}
function template(): ImageTemplate {
  return {
    id: 'tpl',
    directory: 'templates/tpl',
    builtin: false,
    data: {
      name: '背包共用规则',
      mode: 'smart',
      style: '白灰背景',
      text_policy: '只写真实卖点',
      slots: [{ id: 'h1', purpose: '主视觉', brief: '主体居中', refs: ['@product.front'] }],
    },
  }
}
function result(id: string): ImageResult {
  return {
    id: `result-${id}`,
    productId: id,
    slotId: 'h1',
    revision: 2,
    path: `results/${id}.png`,
    width: 1024,
    height: 1024,
    remoteId: null,
    error: null,
    prompt: 'image prompt',
    review: null,
    config,
  }
}
function task(): ImageTask {
  const tpl = template()
  return {
    id: 'workflow',
    revision: 2,
    createdAt: '',
    updatedAt: '',
    brief: {
      ...emptyBrief('workflow'),
      name: '背包制作流程',
      requirement: '制作可重复换品的背包套图',
      count: 1,
      templateId: 'tpl',
      workflowInput: { mode: 'smart', sources: [source] },
      products: [product('a')],
    },
    status: 'ready',
    progress: '试品已保存',
    error: null,
    approvedGroups: [],
    templates: [tpl],
    plans: [
      {
        productId: 'a',
        slotId: 'h1',
        purpose: '主视觉',
        copy: '',
        prompt: 'image prompt',
        refs: ['assets/a.png'],
      },
    ],
    results: [result('a')],
    workflow: {
      ruleVersion: 1,
      rulesCurrent: true,
      approvedVersion: null,
      sampleIds: ['a'],
      changes: [
        {
          version: 1,
          revision: 2,
          note: '初次制作',
          summary: '统一留白和构图',
          createdAt: '2026-09-09T01:00:00Z',
          template: tpl,
        },
      ],
    },
  }
}
function load(t: ImageTask) {
  localStorage.setItem(
    'dsivio-image-draft-v1',
    JSON.stringify({ taskId: t.id, revision: t.revision, brief: t.brief }),
  )
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue({
    tasks: [t],
    templates: t.templates,
    config,
    providers: [],
  })
  vi.mocked(api.imageStudioGet).mockResolvedValue(t)
}

beforeEach(() => {
  vi.clearAllMocks()
  localStorage.clear()
  vi.mocked(api.imageStudioBootstrap).mockResolvedValue({
    tasks: [],
    templates: [],
    config,
    providers: [],
  })
  vi.mocked(api.imageStudioPreview).mockResolvedValue('data:image/png;base64,aW1hZ2U=')
})

describe('制作、试品、反馈和持续出图', () => {
  it('preserves the existing feature entries and exports to the task folder without another picker', async () => {
    const initial = task()
    initial.outputDirectory = 'C:/Images/backpack'
    load(initial)
    vi.mocked(api.imageStudioExport).mockResolvedValue('C:/Images/backpack/deliveries/batch')
    render(<ImageStudio />)
    const nav = screen.getByRole('navigation', { name: '图片功能' })
    expect(within(nav).getAllByRole('button')).toHaveLength(7)
    fireEvent.click(await screen.findByRole('button', { name: '导出本版图片' }))
    expect(screen.getByLabelText('交付保存位置')).toHaveValue('C:/Images/backpack/deliveries')
    fireEvent.click(screen.getByRole('button', { name: '导出到任务文件夹' }))
    await waitFor(() => expect(api.imageStudioExport).toHaveBeenCalledWith('workflow', '', 800, 800, 2048))
    expect(open).not.toHaveBeenCalled()
    expect(await screen.findByText('已导出到 C:/Images/backpack/deliveries/batch')).toBeInTheDocument()
  })
  it('accepts source material separately from trial products and starts rule creation in one action', async () => {
    const made = task()
    made.brief.products = []
    made.results = []
    made.plans = []
    made.workflow!.sampleIds = []
    vi.mocked(open).mockResolvedValue(['C:/source.png'])
    vi.mocked(api.imageStudioImport).mockResolvedValue([{ ...product('source'), assets: [source] }])
    vi.mocked(api.imageStudioSave).mockImplementation(async (brief: ImageBrief) => ({
      ...made,
      brief,
    }))
    vi.mocked(api.imageStudioAction).mockResolvedValue(made)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '制作与试品' }))
    expect(screen.getByRole('radio', { name: '商品照片' })).toBeChecked()
    expect(screen.getByText('新做一套')).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: '现成套图' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '商品图起步' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: '成套样图起步' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('任务名称')).toHaveAttribute('placeholder', '通用电商 · 巴西市场')
    fireEvent.click(screen.getByRole('button', { name: '选择商品照片' }))
    await waitFor(() => expect(screen.getByText('1. 原始商品.png')).toBeInTheDocument())
    fireEvent.change(screen.getByLabelText('制作要求'), {
      target: { value: made.brief.requirement },
    })
    fireEvent.click(screen.getByRole('button', { name: '开始制作' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith('workflow', 2, {
        kind: 'workflow_build',
        group: '未分类',
      }),
    )
    expect(api.imageStudioSave).toHaveBeenCalledWith(
      expect.objectContaining({
        feature: 'workflow',
        name: '通用电商 · 巴西市场',
        products: [],
        workflowInput: { mode: 'smart', sources: [source] },
      }),
      undefined,
      undefined,
    )
    expect(
      await screen.findByRole('heading', { name: '已制作 · 背包共用规则' }),
    ).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '生成所选试品' })).toBeDisabled()
  })

  it('restores a completed trial, confirms it and saves appended products into the same workflow', async () => {
    const initial = task()
    load(initial)
    const approved = { ...initial, workflow: { ...initial.workflow!, approvedVersion: 1 } }
    vi.mocked(api.imageStudioAction).mockResolvedValue(approved)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '试品满意，确认这版' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        2,
        expect.objectContaining({ kind: 'workflow_approve' }),
      ),
    )
    expect(await screen.findByRole('button', { name: '生成新增商品' })).toBeDisabled()
    vi.mocked(open).mockResolvedValue(['C:/b-front.png', 'C:/b-back.png'])
    vi.mocked(api.imageStudioImport).mockResolvedValue([product('b')])
    fireEvent.click(screen.getByRole('button', { name: '添加一款商品' }))
    await waitFor(() => expect(screen.getByRole('button', { name: '生成新增商品' })).toBeEnabled())
    vi.mocked(api.imageStudioSave).mockImplementation(async (brief: ImageBrief) => ({
      ...approved,
      brief,
      revision: 3,
    }))
    fireEvent.click(screen.getByRole('button', { name: '生成新增商品' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        3,
        expect.objectContaining({ kind: 'workflow_produce' }),
      ),
    )
    expect(api.imageStudioSave).toHaveBeenCalledWith(
      expect.objectContaining({
        products: [product('a'), product('b')],
        workflowInput: initial.brief.workflowInput,
      }),
      'workflow',
      2,
    )
    expect(api.imageStudioImport).toHaveBeenCalledWith(['C:/b-front.png', 'C:/b-back.png'], false)
  })

  it('sends shared feedback with trial context and blocks approval until the new version has trial results', async () => {
    const initial = task()
    load(initial)
    const next = structuredClone(initial)
    next.revision = 3
    next.workflow!.ruleVersion = 2
    next.workflow!.approvedVersion = null
    next.workflow!.changes.push({
      ...next.workflow!.changes[0],
      version: 2,
      revision: 3,
      note: '产品放大，后续都用这个比例',
      summary: '共用规则已更新',
    })
    next.plans = []
    vi.mocked(api.imageStudioAction).mockResolvedValue(next)
    render(<ImageStudio />)
    const feedback = await screen.findByLabelText('对共用规则的修改意见')
    fireEvent.change(feedback, { target: { value: '产品放大，后续都用这个比例' } })
    fireEvent.click(screen.getByRole('button', { name: '修改共用规则并重试' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        2,
        expect.objectContaining({ kind: 'workflow_refine', note: '产品放大，后续都用这个比例' }),
      ),
    )
    expect(screen.getByRole('button', { name: '试品满意，确认这版' })).toBeDisabled()
    expect(screen.getByText('v2 · 待试品确认')).toBeInTheDocument()
    fireEvent.click(screen.getByText('修改记录与历史图片'))
    fireEvent.click(screen.getByRole('button', { name: '查看历史图片（1 张）' }))
    expect(screen.getByText('商品 a · h1 · v1')).toBeInTheDocument()
  })

  it('submits selected products for trial and requires all selected pages before approval', async () => {
    const initial = task()
    initial.brief.products.push(product('b'))
    load(initial)
    vi.mocked(api.imageStudioAction).mockResolvedValue({
      ...initial,
      workflow: { ...initial.workflow!, sampleIds: ['a', 'b'] },
    })
    render(<ImageStudio />)
    await waitFor(() =>
      expect(screen.getAllByRole('checkbox', { name: '选作试品' })[1]).toBeEnabled(),
    )
    fireEvent.click(screen.getAllByRole('checkbox', { name: '选作试品' })[1])
    expect(screen.getByRole('button', { name: '试品满意，确认这版' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: '生成所选试品' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        2,
        expect.objectContaining({ kind: 'workflow_trial', sampleIds: ['a', 'b'] }),
      ),
    )
    expect(screen.getByRole('button', { name: '试品满意，确认这版' })).toBeDisabled()
  })

  it('edits common rules without rebinding assets and keeps single-image editing separate', async () => {
    const initial = task()
    load(initial)
    vi.mocked(api.imageStudioAction).mockResolvedValue(initial)
    render(<ImageStudio />)
    fireEvent.click(await screen.findByText('查看与编辑共用规则'))
    fireEvent.change(screen.getByLabelText('统一风格'), { target: { value: '暖灰背景，深蓝标题' } })
    fireEvent.click(screen.getByRole('button', { name: '保存规则并重试' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        2,
        expect.objectContaining({
          kind: 'workflow_edit',
          templateData: { ...initial.templates[0].data, style: '暖灰背景，深蓝标题' },
        }),
      ),
    )
    fireEvent.click(screen.getByRole('button', { name: '查看 / 只改这张' }))
    const dialog = screen.getByRole('dialog', { name: '查看和修改图片' })
    expect(within(dialog).getByLabelText('只修改这一张')).toBeInTheDocument()
    expect(within(dialog).queryByRole('button', { name: '冻结为样图模板' })).not.toBeInTheDocument()
  })

  it('resumes persisted remote images and disables shared refinement while they are unresolved', async () => {
    const initial = task()
    initial.results[0] = {
      ...initial.results[0],
      path: null,
      remoteId: 'remote-existing',
      error: '等待超时',
    }
    load(initial)
    vi.mocked(api.imageStudioAction).mockResolvedValue(initial)
    render(<ImageStudio />)
    fireEvent.change(await screen.findByLabelText('对共用规则的修改意见'), {
      target: { value: '修改标题大小' },
    })
    expect(screen.getByRole('button', { name: '修改共用规则并重试' })).toBeDisabled()
    fireEvent.click(screen.getByRole('button', { name: '恢复查询' }))
    await waitFor(() =>
      expect(api.imageStudioAction).toHaveBeenCalledWith(
        'workflow',
        2,
        expect.objectContaining({ kind: 'resume', resultId: 'result-a' }),
      ),
    )
    expect(api.imageStudioAction).toHaveBeenCalledTimes(1)
  })

  it('invalidates the visible gate when source requirements change', async () => {
    load(task())
    render(<ImageStudio />)
    fireEvent.click(await screen.findByText('01 · 原始素材'))
    fireEvent.change(screen.getByLabelText('制作要求'), { target: { value: '换个全新的版式' } })
    expect(screen.getByRole('button', { name: '生成所选试品' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '试品满意，确认这版' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '更新制作' })).toBeEnabled()
  })

  it('keeps the requirement field as a plain writing box', async () => {
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '制作与试品' }))
    const field = screen.getByLabelText('制作要求')
    expect(field.closest('.is-req')).toBeTruthy()
    expect(field.previousElementSibling).toHaveClass('is-req-bar')
    expect(field).toHaveAttribute('placeholder', '写清版式、背景、文字和以后换品怎么沿用')
    expect(screen.queryByRole('button', { name: '统一白灰背景' })).not.toBeInTheDocument()
  })

  it('switches the drop zone copy when starting from a finished set', async () => {
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '制作与试品' }))
    fireEvent.click(screen.getByRole('radio', { name: '现成套图' }))
    expect(screen.getByRole('button', { name: '选择现成套图' })).toBeInTheDocument()
    expect(screen.getByText('按页序把套图拖到这里')).toBeInTheDocument()
    expect(screen.getByText('只换商品')).toBeInTheDocument()
    expect(screen.getByLabelText('原始参考投放区')).toBeInTheDocument()
  })

  it('imports dropped files as original references instead of trial products', async () => {
    vi.mocked(api.imageStudioImport).mockResolvedValue([{ ...product('source'), assets: [source] }])
    render(<ImageStudio />)
    fireEvent.click(await screen.findByRole('button', { name: '制作与试品' }))
    const zone = screen.getByLabelText('原始参考投放区')
    await waitFor(() => expect(dropHandler).toBeTypeOf('function'))
    dropHandler?.({ payload: { type: 'enter' } })
    await waitFor(() => expect(zone).toHaveClass('is-drop-active'))
    dropHandler?.({ payload: { type: 'drop', paths: ['C:\\source.png'] } })
    await waitFor(() =>
      expect(api.imageStudioImport).toHaveBeenCalledWith(['C:\\source.png'], false),
    )
    expect(await screen.findByText('1. 原始商品.png')).toBeInTheDocument()
    expect(screen.queryByLabelText('商品名称')).not.toBeInTheDocument()
  })
})

describe('workflow trial gate', () => {
  it('requires the current successful attempt for every rule slot of every selected existing product', () => {
    const initial = task()
    expect(workflowSamplesComplete(initial)).toBe(true)
    initial.results.push({ ...result('a'), id: 'failed-edit', path: null })
    expect(workflowSamplesComplete(initial)).toBe(false)
    initial.results.pop()
    initial.templates[0].data.slots.push({ id: 'h2', brief: '细节图' })
    expect(workflowSamplesComplete(initial)).toBe(false)
    initial.templates[0].data.slots.pop()
    initial.results[0].revision = 1
    expect(workflowSamplesComplete(initial)).toBe(false)
    initial.results[0].revision = 2
    initial.brief.products = []
    expect(workflowSamplesComplete(initial)).toBe(false)
  })
})
