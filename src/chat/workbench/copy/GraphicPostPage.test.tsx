import { beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from '../../../api/tauri'
import { emptyBrief, type ImageTask } from '../image/projects/types'
import { GraphicPostPage } from './GraphicPostPage'

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
    ...emptyBrief('post'),
    name: '图文',
    requirement: '轻便透气',
    platform: 'xhs',
    ratio: '3:4',
    products: [{
      id: 'sku',
      name: '通勤包',
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
    plans: [{
      productId: 'sku',
      slotId: 'h1',
      purpose: '封面',
      copy: '# 轻便通勤\n\n夏天也能背一天。\n\n#通勤',
      prompt: 'cover prompt',
      refs: ['assets/front.png'],
    }, {
      productId: 'sku',
      slotId: 'h2',
      purpose: '细节',
      copy: '',
      prompt: 'detail prompt',
      refs: ['assets/front.png'],
    }],
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

describe('图文带货', () => {
  it('assembles a post brief and asks the agent to plan before any image is generated', async () => {
    const saved = task()
    vi.mocked(api.workbenchImageSave).mockResolvedValue(saved)
    vi.mocked(api.workbenchImageAction).mockResolvedValue({ ...saved, plans: saved.plans, status: 'ready' })
    vi.mocked(open).mockResolvedValue(['/tmp/front.png'])
    render(<GraphicPostPage />)
    fireEvent.click(await screen.findByRole('button', { name: '选择图片' }))
    fireEvent.change(await screen.findByLabelText('带货需求描述'), { target: { value: '轻便透气' } })
    fireEvent.click(screen.getByRole('button', { name: '生成文案方案' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 2, { kind: 'start', group: '未分类' }, config))
    expect(api.workbenchImageSave).toHaveBeenCalledWith(
      expect.objectContaining({ feature: 'post', requirement: '轻便透气', platform: 'xhs', ratio: '3:4', count: 4 }),
      undefined,
      undefined,
      config,
    )
    expect(await screen.findByRole('tab', { name: /方案确认/ })).toHaveAttribute('aria-selected', 'true')
  })

  it('saves an edited post and then generates every planned image', async () => {
    const current = task()
    localStorage.setItem('dsivio-image-draft-v1:feature:post', JSON.stringify({ brief: current.brief, taskId: current.id, revision: current.revision, stage: 'plan', plans: current.plans }))
    vi.mocked(api.workbenchImageBootstrap).mockResolvedValue({ tasks: [current], templates: [], config, providers: [{ id: 'p', name: 'Test', models: ['gemini-image'], ready: true }] })
    vi.mocked(api.workbenchImageSavePlans).mockImplementation(async (_id, _revision, plans) => ({ ...current, revision: 3, plans }))
    vi.mocked(api.workbenchImageAction).mockResolvedValue({ ...current, revision: 3, status: 'running' })
    render(<GraphicPostPage />)
    const copy = await screen.findByLabelText('发布文案')
    fireEvent.change(copy, { target: { value: '# 改过的标题\n\n改过的正文\n\n#轻便' } })
    fireEvent.click(screen.getByRole('button', { name: '保存方案' }))
    await waitFor(() => expect(api.workbenchImageSavePlans).toHaveBeenCalled())
    const savedPlans = vi.mocked(api.workbenchImageSavePlans).mock.calls[0][2]
    expect(savedPlans.find((plan) => plan.slotId === 'h1')?.copy).toContain('改过的标题')
    expect(savedPlans.find((plan) => plan.slotId === 'h2')?.prompt).toBe('detail prompt')
    fireEvent.click(screen.getByRole('button', { name: '确认并生成配图' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 3, { kind: 'generate', group: '未分类' }, config))
  })

  it('retries only the failed image', async () => {
    const current = task({
      results: [{
        id: 'h2-fail',
        productId: 'sku',
        slotId: 'h2',
        revision: 2,
        path: null,
        error: '图片接口 HTTP 429',
        remoteId: null,
        prompt: 'detail prompt',
        width: 0,
        height: 0,
        review: null,
        config,
      }],
    })
    localStorage.setItem('dsivio-image-draft-v1:feature:post', JSON.stringify({ brief: current.brief, taskId: current.id, revision: current.revision, stage: 'results' }))
    vi.mocked(api.workbenchImageBootstrap).mockResolvedValue({ tasks: [current], templates: [], config, providers: [{ id: 'p', name: 'Test', models: ['gemini-image'], ready: true }] })
    vi.mocked(api.workbenchImageGet).mockResolvedValue(current)
    vi.mocked(api.workbenchImageAction).mockResolvedValue(current)
    render(<GraphicPostPage />)
    fireEvent.click(await screen.findByRole('button', { name: '单页重试' }))
    await waitFor(() => expect(api.workbenchImageAction).toHaveBeenCalledWith('job', 2, {
      kind: 'retry',
      productId: 'sku',
      slotId: 'h2',
      group: '未分类',
    }, config))
    expect(api.workbenchImageAction).toHaveBeenCalledTimes(1)
  })

  it('restores the post draft after leaving and reopening', async () => {
    render(<GraphicPostPage />)
    fireEvent.change(await screen.findByLabelText('带货需求描述'), { target: { value: '离开前的卖点' } })
    await waitFor(() => expect(localStorage.getItem('dsivio-image-draft-v1:feature:post')).toContain('离开前的卖点'))
    cleanup()
    render(<GraphicPostPage />)
    expect(await screen.findByLabelText('带货需求描述')).toHaveValue('离开前的卖点')
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('studio_draft', expect.objectContaining({ domain: 'image', entry: 'post' })))
    expect(api.workbenchImageAction).not.toHaveBeenCalled()
  })
})
