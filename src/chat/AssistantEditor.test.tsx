import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { AssistantEditor } from './AssistantEditor'
import { chatApi } from './api'
vi.mock('./api', () => ({ chatApi: { createAssistant: vi.fn(async a => a), updateAssistant: vi.fn(async a => a) } }))
describe('prompt-only assistant editor', () => {
  it('saves a reusable prompt with no model or tool bindings', async () => {
    const user = userEvent.setup()
    const saved = vi.fn()
    render(<AssistantEditor onSaved={saved} onCancel={() => {}} />)
    expect(screen.getByRole('button', { name: '保存助手' })).toBeDisabled()
    fireEvent.change(screen.getByLabelText('助手名称'), { target: { value: ' 商品摄影 ' } })
    await user.click(screen.getByRole('button', { name: '助手分类' }))
    await user.click(screen.getByRole('option', { name: '图片' }))
    fireEvent.change(screen.getByLabelText('助手描述'), { target: { value: ' 突出商品材质与轮廓。 ' } })
    fireEvent.change(screen.getByLabelText('系统提示词'), { target: { value: ' 用柔和侧光表现商品材质。 ' } })
    fireEvent.click(screen.getByRole('button', { name: '保存助手' }))
    await waitFor(() => expect(saved).toHaveBeenCalled())
    expect(chatApi.createAssistant).toHaveBeenCalledWith(expect.objectContaining({ name: '商品摄影', category: 'image', description: '突出商品材质与轮廓。', system_prompt: '用柔和侧光表现商品材质。', model: '', provider_id: '', mcp_server_ids: [], skill_ids: [], installed: false }))
  })
  it('retains the draft and reports a failed save', async () => {
    vi.mocked(chatApi.createAssistant).mockRejectedValueOnce(new Error('同名助手已存在'))
    render(<AssistantEditor onSaved={() => {}} onCancel={() => {}} />)
    fireEvent.change(screen.getByLabelText('助手名称'), { target: { value: '摄影' } })
    fireEvent.change(screen.getByLabelText('系统提示词'), { target: { value: '规则' } })
    fireEvent.click(screen.getByRole('button', { name: '保存助手' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('同名助手已存在')
    expect(screen.getByLabelText('系统提示词')).toHaveValue('规则')
  })
})
