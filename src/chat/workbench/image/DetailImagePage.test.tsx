import { beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import { emptyBrief, type ImageTask } from './projects/types'
import { DetailImagePage } from './DetailImagePage'

vi.mock('../../../api/tauri', () => ({
  isTauriRuntime: () => true,
  api: {
    studioTaskLibrary: vi.fn(async () => ({})),
    workbenchImageBootstrap: vi.fn(),
    workbenchImageGet: vi.fn(),
    workbenchImageSave: vi.fn(),
    workbenchImageAction: vi.fn(),
    workbenchImagePreview: vi.fn(),
    workbenchImageSavePlans: vi.fn(),
    workbenchImageImport: vi.fn(),
  },
}))
vi.mock('../../../api/settingsCache', () => ({
  getSettingsCached: vi.fn().mockResolvedValue({ providers: [] }),
  subscribeSettings: vi.fn(() => () => {}),
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ revision: 0, value: null })) }))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({ onDragDropEvent: () => Promise.resolve(() => {}) }),
}))

const config = {
  providerId: 'p',
  model: 'gemini-image',
  protocol: 'gemini',
  agentProviderId: '',
  agentModel: '',
}

function task(extra: Partial<ImageTask> = {}): ImageTask {
  const brief = {
    ...emptyBrief('detail'),
    name: '详情页',
    requirement: '透气网面',
    platform: 'tb',
    ratio: '3:4',
    products: [{
      id: 'sku',
      name: '童鞋',
      category: '未分类',
      kind: '',
      facts: '',
      front: 'asset-1',
      back: null,
      assets: [{ id: 'asset-1', name: 'front.png', path: 'assets/front.png' }],
      templateId: null,
    }],
  }
  return {
    id: 'job',
    revision: 2,
    createdAt: '',
    updatedAt: '',
    brief,
    plans: [
      { productId: 'sku', slotId: 'hero', purpose: '头图', copy: '透气', prompt: 'hero prompt', refs: ['assets/front.png'] },
      { productId: 'sku', slotId: 'selling', purpose: '卖点', copy: '网面', prompt: 'selling prompt', refs: ['assets/front.png'] },
    ],
    results: [],
    approvedGroups: [],
    status: 'ready',
    progress: '',
    error: null,
    templates: [],
    ...extra,
  }
}

beforeEach(() => {
  vi.clearAllMocks()
  localStorage.clear()
  localStorage.setItem('dsivio.workbench.image-project-settings', JSON.stringify(config))
  vi.mocked(api.workbenchImageBootstrap).mockResolvedValue({
    tasks: [],
    templates: [],
    config,
    providers: [{ id: 'p', name: 'Test', models: ['gemini-image'], ready: true }],
  })
  vi.mocked(api.workbenchImagePreview).mockResolvedValue('data:image/png;base64,iVBORw0KGgo=')
  vi.mocked(api.workbenchImageImport).mockResolvedValue(task().brief.products)
})

describe('详情页生成', () => {
  it('assembles a detail brief and plans modules before generating', async () => {
    const saved = task()
    vi.mocked(api.workbenchImageSave).mockResolvedValue(saved)
    vi.mocked(api.workbenchImageAction).mockResolvedValue(saved)
    vi.mocked(open).mockResolvedValue(['/tmp/front.png'])
    render(<DetailImagePage />)
    fireEvent.click(await screen.findByRole('button', { name: '选择图片' }))
    fireEvent.change(await screen.findByLabelText('产品描述'), { target: { value: '透气网面' } })
    fireEvent.click(screen.getByRole('button', { name: '规划详情模块' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 2, { kind: 'start', group: '未分类' }, config))
    expect(api.workbenchImageSave).toHaveBeenCalledWith(
      expect.objectContaining({ feature: 'detail', requirement: '透气网面', platform: 'tb', ratio: '3:4', count: 5 }),
      undefined,
      undefined,
      config,
    )
    expect(await screen.findByRole('tab', { name: /方案确认/ })).toHaveAttribute('aria-selected', 'true')
  })

  it('edits one module prompt and generates the planned modules', async () => {
    const current = task()
    localStorage.setItem('dsivio-image-draft-v1:feature:detail', JSON.stringify({ brief: current.brief, taskId: current.id, revision: current.revision, stage: 'plan', plans: current.plans }))
    vi.mocked(api.workbenchImageBootstrap).mockResolvedValue({ tasks: [current], templates: [], config, providers: [{ id: 'p', name: 'Test', models: ['gemini-image'], ready: true }] })
    vi.mocked(api.workbenchImageSavePlans).mockImplementation(async (_id, _revision, plans) => ({ ...current, revision: 3, plans }))
    vi.mocked(api.workbenchImageAction).mockResolvedValue({ ...current, revision: 3, status: 'running' })
    render(<DetailImagePage />)
    fireEvent.change(await screen.findByLabelText('头图文案'), { target: { value: '更透气' } })
    fireEvent.click(screen.getByRole('button', { name: '保存方案' }))
    await waitFor(() => expect(api.workbenchImageSavePlans).toHaveBeenCalled())
    expect(vi.mocked(api.workbenchImageSavePlans).mock.calls[0][2].find((plan) => plan.slotId === 'hero')?.copy).toBe('更透气')
    fireEvent.click(screen.getByRole('button', { name: '生成模块' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 3, { kind: 'generate', group: '未分类' }, config))
  })

  it('retries only the failed module', async () => {
    const current = task({
      results: [{
        id: 'selling-fail',
        productId: 'sku',
        slotId: 'selling',
        revision: 2,
        path: null,
        error: '上游生图暂时失败',
        remoteId: null,
        prompt: 'selling prompt',
        width: 0,
        height: 0,
        review: null,
        config,
      }],
    })
    localStorage.setItem('dsivio-image-draft-v1:feature:detail', JSON.stringify({ brief: current.brief, taskId: current.id, revision: current.revision, stage: 'results' }))
    vi.mocked(api.workbenchImageBootstrap).mockResolvedValue({ tasks: [current], templates: [], config, providers: [{ id: 'p', name: 'Test', models: ['gemini-image'], ready: true }] })
    vi.mocked(api.workbenchImageAction).mockResolvedValue(current)
    render(<DetailImagePage />)
    fireEvent.click(await screen.findByRole('button', { name: '单页重试' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 2, {
      kind: 'retry',
      productId: 'sku',
      slotId: 'selling',
      group: '未分类',
    }, config))
  })

  it('restores the detail draft after leaving and reopening', async () => {
    render(<DetailImagePage />)
    fireEvent.change(await screen.findByLabelText('产品描述'), { target: { value: '离开前的描述' } })
    await waitFor(() => expect(localStorage.getItem('dsivio-image-draft-v1:feature:detail')).toContain('离开前的描述'))
    cleanup()
    render(<DetailImagePage />)
    expect(await screen.findByLabelText('产品描述')).toHaveValue('离开前的描述')
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('studio_draft', expect.objectContaining({ domain: 'image', entry: 'detail' })))
  })
})