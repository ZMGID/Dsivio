import { beforeEach, describe, expect, it, vi } from 'vitest'
import { api, isTauriRuntime } from '../../../api/tauri'
import { blankWorkflow } from './workflowModel'
import { graphProblems } from './workflowGraph'
import { checkWorkflow } from './workflowValidation'

vi.mock('../../../api/tauri', () => ({
  isTauriRuntime: vi.fn(() => false),
  api: { checkWorkflow: vi.fn() },
}))

const incomplete = {
  ...blankWorkflow('browser'),
  nodes: [{ id: 'prompt', kind: 'prompt.input' as const, title: '输入', position: { x: 0, y: 0 }, config: { type: 'prompt' as const, text: '' } }],
}

describe('workflow check authority', () => {
  beforeEach(() => {
    vi.mocked(isTauriRuntime).mockReturnValue(false)
    vi.mocked(api.checkWorkflow).mockReset()
  })

  it('returns backend issues in the desktop app without adding local model or asset rules', async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true)
    vi.mocked(api.checkWorkflow).mockResolvedValue(['生成：模型已不在可用模型池中'])
    await expect(checkWorkflow(incomplete)).resolves.toEqual(['生成：模型已不在可用模型池中'])
    expect(api.checkWorkflow).toHaveBeenCalledWith(incomplete)
  })

  it('reports a rejected backend check as an issue', async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true)
    vi.mocked(api.checkWorkflow).mockRejectedValue(new Error('offline'))
    await expect(checkWorkflow(incomplete)).resolves.toEqual(['配置检查失败：Error: offline'])
  })

  it('uses local graph problems only in the browser', async () => {
    await expect(checkWorkflow(incomplete)).resolves.toEqual(graphProblems(incomplete))
    expect(api.checkWorkflow).not.toHaveBeenCalled()
    expect(graphProblems(incomplete).join()).toContain('填写提示词')
  })
})
