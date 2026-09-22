import { act, fireEvent, render, screen, waitFor, within, cleanup } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { getSettingsCached, updateSettingsCached } from '../../../api/settingsCache'
import { PickLibraryPage } from './PickLibraryPage'
import { LookalikePage } from './LookalikePage'
import { productDraft } from './productDraft'
import type { PickItem, SourcingProduct, SourcingSearch } from '../../../generated/sourcing'

vi.mock('../../../api/tauri', () => ({ api: { sourcingHistory: vi.fn(), sourcingGetSearch: vi.fn(), sourcingSearch: vi.fn(), sourcingSavePick: vi.fn(), sourcingListPicks: vi.fn(), sourcingDeletePick: vi.fn(), openExternal: vi.fn() } }))
vi.mock('../../../api/settingsCache', () => ({ getSettingsCached: vi.fn(), updateSettingsCached: vi.fn(), subscribeSettings: vi.fn(() => () => {}) }))
const product: SourcingProduct = { id: '123', title: '同款杯子', url: 'https://detail.1688.com/offer/123.html', imageUrl: null, price: '12.8', supplier: '杯子工厂', skuId: '456', skuTitle: '白色', minimumOrder: '2', soldCount: null, stock: null, relevance: 0.99 }
const result: SourcingSearch = { id: 'search1', name: '杯子.png', fetchedAt: '2026-09-22T00:00:00Z', sort: 'relevance', purchaseAmount: 1, products: [product] }
const item: PickItem = { id: 'pick1', revision: 1, createdAt: '', updatedAt: '', product: productDraft(product) }
afterEach(cleanup)
beforeEach(() => {
  vi.clearAllMocks(); window.history.replaceState(null, '', '/#chat/workbench/picks')
  vi.mocked(getSettingsCached).mockResolvedValue({ sourcing: { alibabaAk: 'configured' } } as Awaited<ReturnType<typeof getSettingsCached>>)
  vi.mocked(api.sourcingHistory).mockResolvedValue([{ id: result.id, name: result.name, fetchedAt: result.fetchedAt, count: 1 }])
  vi.mocked(api.sourcingGetSearch).mockResolvedValue(result)
  vi.mocked(api.sourcingListPicks).mockResolvedValue({ items: [item], total: 1 })
  vi.mocked(api.sourcingSavePick).mockResolvedValue(item)
})
describe('sourcing pages', () => {
  it('hides stale products when loading a new filter fails and recovers on retry', async () => {
    render(<PickLibraryPage />)
    await screen.findByText('同款杯子')
    vi.mocked(api.sourcingListPicks).mockRejectedValueOnce('读取选品失败')
    fireEvent.change(screen.getByLabelText('搜索名称、标签或备注'), { target: { value: '背包' } })
    expect(await screen.findByRole('alert')).toHaveTextContent('读取选品失败')
    expect(screen.queryByText('同款杯子')).not.toBeInTheDocument()
    vi.mocked(api.sourcingListPicks).mockResolvedValueOnce({ items: [], total: 0 })
    fireEvent.click(screen.getByText('刷新'))
    expect(await screen.findByText('没有符合筛选条件的选品')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows an unsaved-history warning while keeping returned products usable', async () => {
    vi.mocked(api.sourcingGetSearch).mockResolvedValueOnce({ ...result, historyWarning: '搜索记录未保存：磁盘空间不足' })
    render(<LookalikePage />)
    fireEvent.click(screen.getByText('搜索记录'))
    fireEvent.click(await screen.findByText('查看结果'))
    expect(await screen.findByRole('alert')).toHaveTextContent('搜索记录未保存')
    expect(screen.getByText('同款杯子')).toBeInTheDocument()
    expect(screen.getByText('加入选品库')).not.toBeDisabled()
    expect(api.sourcingSearch).not.toHaveBeenCalled()
  })

  it('can retry a failed initial settings read', async () => {
    vi.mocked(getSettingsCached).mockRejectedValueOnce('设置读取失败')
    render(<LookalikePage />)
    expect(await screen.findByRole('alert')).toHaveTextContent('设置读取失败')
    fireEvent.click(screen.getByText('重试读取设置'))
    await waitFor(() => expect(screen.getByText('1688 接口设置')).not.toBeDisabled())
    expect(screen.queryByText('设置读取失败')).not.toBeInTheDocument()
  })
  it('restores the saved AK after dismissing unsaved credential edits', async () => {
    render(<LookalikePage />)
    await waitFor(() => expect(screen.getByText('1688 接口设置')).not.toBeDisabled())
    fireEvent.click(screen.getByText('1688 接口设置'))
    fireEvent.change(screen.getByLabelText('1688 AK'), { target: { value: 'unsaved' } })
    fireEvent.click(screen.getByLabelText('关闭'))
    fireEvent.click(screen.getByText('1688 接口设置'))
    expect(screen.getByLabelText('1688 AK')).toHaveValue('configured')
  })

  it('jumps directly to the last valid page for an outdated saved URL', async () => {
    window.history.replaceState(null, '', '/?picksPage=100000#chat/workbench/picks')
    vi.mocked(api.sourcingListPicks).mockResolvedValueOnce({ items: [], total: 1 }).mockResolvedValue({ items: [item], total: 1 })
    render(<PickLibraryPage />)
    expect(await screen.findByText('同款杯子')).toBeInTheDocument()
    expect(api.sourcingListPicks).toHaveBeenCalledTimes(2)
    expect(api.sourcingListPicks).toHaveBeenLastCalledWith(expect.objectContaining({ page: 1 }))
  })

  it('restores persisted search results and collects a product with SKU identity', async () => {
    render(<LookalikePage />)
    fireEvent.click(screen.getByText('搜索记录'))
    fireEvent.click(await screen.findByText('查看结果'))
    expect(await screen.findByText('同款杯子')).toBeInTheDocument()
    fireEvent.click(screen.getByText('加入选品库'))
    await screen.findByText('已收藏')
    expect(api.sourcingSearch).not.toHaveBeenCalled()
    expect(api.sourcingSavePick).toHaveBeenCalledWith({ id: null, expectedRevision: null, product: expect.objectContaining({ source: 'alibaba1688', sourceId: '123:456', url: product.url, price: '12.8' }) })
  })
  it('validates uploads and shows the actual search error without fake results', async () => {
    vi.mocked(api.sourcingSearch).mockRejectedValue('1688 签名校验失败')
    render(<LookalikePage />)
    await waitFor(() => expect(screen.getByText('在 1688 找同款')).not.toBeDisabled())
    const upload = screen.getByLabelText('上传图片', { selector: 'input' })
    fireEvent.change(upload, { target: { files: [new File(['x'], 'bad.txt', { type: 'text/plain' })] } })
    expect(await screen.findByRole('alert')).toHaveTextContent('PNG')
    fireEvent.change(upload, { target: { files: [new File(['test'], 'photo.png', { type: 'image/png' })] } })
    await screen.findByText('photo.png')
    fireEvent.click(screen.getByText('在 1688 找同款'))
    expect(await screen.findByRole('alert')).toHaveTextContent('签名校验失败')
    expect(screen.queryByText('同款杯子')).not.toBeInTheDocument()
    expect(screen.getByText('在 1688 找同款')).not.toBeDisabled()
  })
  it('preserves edits after save errors and sends the expected revision', async () => {
    vi.mocked(api.sourcingSavePick).mockRejectedValueOnce('选品已被修改，请刷新后重试')
    render(<PickLibraryPage />)
    fireEvent.click(await screen.findByText('编辑'))
    const dialog = screen.getByRole('dialog')
    fireEvent.change(within(dialog).getByLabelText('商品名称'), { target: { value: '新杯子' } })
    fireEvent.click(within(dialog).getByText('保存'))
    expect(await screen.findByRole('alert')).toHaveTextContent('已被修改')
    expect(screen.getByDisplayValue('新杯子')).toBeInTheDocument()
    expect(api.sourcingSavePick).toHaveBeenCalledWith(expect.objectContaining({ id: 'pick1', expectedRevision: 1 }))
  })
  it('requires deletion confirmation and prevents duplicate submissions', async () => {
    let resolve!: () => void
    vi.mocked(api.sourcingDeletePick).mockImplementation(() => new Promise<void>(r => { resolve = r }))
    render(<PickLibraryPage />)
    fireEvent.click(await screen.findByText('删除'))
    expect(api.sourcingDeletePick).not.toHaveBeenCalled()
    fireEvent.click(screen.getByText('确认删除'))
    fireEvent.click(screen.getByText('删除中…'))
    expect(api.sourcingDeletePick).toHaveBeenCalledTimes(1)
    expect(api.sourcingDeletePick).toHaveBeenCalledWith('pick1', 1)
    await act(async () => resolve())
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
  })
  it('saves authorization through the existing settings owner', async () => {
    vi.mocked(updateSettingsCached).mockResolvedValue({} as Awaited<ReturnType<typeof updateSettingsCached>>)
    render(<LookalikePage />)
    await waitFor(() => expect(screen.getByText('1688 接口设置')).not.toBeDisabled())
    fireEvent.click(screen.getByText('1688 接口设置'))
    fireEvent.change(screen.getByLabelText('1688 AK'), { target: { value: 'new-ak' } })
    fireEvent.click(within(screen.getByRole('dialog')).getByText('保存'))
    await screen.findByText('接口设置已保存')
    const update = vi.mocked(updateSettingsCached).mock.calls[0][0]
    expect(update({ unrelated: 42 } as never)).toEqual({ unrelated: 42, sourcing: { alibabaAk: 'new-ak' } })
  })
})
