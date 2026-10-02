import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { ListingCheckPage } from './ListingCheckPage'

vi.mock('../../../api/tauri', () => ({
  api: { runAiTask: vi.fn(), cancelAiTask: vi.fn() },
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }))

beforeEach(() => {
  vi.mocked(api.runAiTask).mockReset()
  vi.mocked(api.cancelAiTask).mockReset()
})

it('offers the overseas and domestic platforms and reports a Shopee title that is too long', () => {
  render(<ListingCheckPage />)
  fireEvent.click(screen.getByRole('button', { name: '检查平台' }))
  for (const name of ['淘宝', '拼多多', '虾皮 Shopee', '希音 SHEIN', 'TikTok Shop', '美客多 Mercado Libre']) {
    expect(screen.getByRole('option', { name })).toBeInTheDocument()
  }
  fireEvent.click(screen.getByRole('option', { name: '虾皮 Shopee' }))
  fireEvent.change(screen.getByPlaceholderText('品牌 + 品名 + 关键属性'), { target: { value: 'a'.repeat(101) } })
  fireEvent.click(screen.getByRole('button', { name: '开始检查' }))
  expect(screen.getByText(/超出上限/)).toBeInTheDocument()
})

it('shows an AI suggestion without writing it into the title', async () => {
  vi.mocked(api.runAiTask).mockResolvedValue({ text: '把标题写具体一点', toolCalls: [], usage: null })
  render(<ListingCheckPage />)
  const title = screen.getByPlaceholderText('品牌 + 品名 + 关键属性')
  fireEvent.change(title, { target: { value: '纯棉抗菌男士短袖T恤 夏季宽松白色' } })
  fireEvent.click(screen.getByRole('button', { name: 'AI 建议' }))
  expect(await screen.findByText('把标题写具体一点')).toBeInTheDocument()
  expect(title).toHaveValue('纯棉抗菌男士短袖T恤 夏季宽松白色')
  expect(api.runAiTask).toHaveBeenCalledTimes(1)
  expect(vi.mocked(api.runAiTask).mock.calls[0][0].mode).toBe('once')
})

it('drops a suggestion that arrives after the title changes', async () => {
  let resolveRun: (value: { text: string; toolCalls: []; usage: null }) => void = () => {}
  vi.mocked(api.runAiTask).mockImplementation(() => new Promise((resolve) => { resolveRun = resolve }))
  render(<ListingCheckPage />)
  const title = screen.getByPlaceholderText('品牌 + 品名 + 关键属性')
  fireEvent.change(title, { target: { value: '纯棉抗菌男士短袖T恤 夏季宽松白色' } })
  fireEvent.click(screen.getByRole('button', { name: 'AI 建议' }))
  await waitFor(() => expect(api.runAiTask).toHaveBeenCalledTimes(1))
  fireEvent.change(title, { target: { value: '纯棉圆领短袖' } })
  await act(async () => {
    resolveRun({ text: '迟到的建议', toolCalls: [], usage: null })
  })
  expect(screen.queryByText('迟到的建议')).toBeNull()
  expect(title).toHaveValue('纯棉圆领短袖')
})
