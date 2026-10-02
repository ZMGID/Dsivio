import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api, type ShopConnection } from '../../../api/tauri'
import type { Category, CommerceCapabilities, ListingRecord } from '../../../generated/commerce'
import type { Product } from '../../../generated/products'
import { ListingPage } from './ListingPage'

vi.mock('../../../api/tauri', () => ({
  api: {
    commerceShops: vi.fn(), productsList: vi.fn(), commerceCapabilities: vi.fn(), commerceCategories: vi.fn(),
    commerceAttributes: vi.fn(), commerceListings: vi.fn(), commerceSubmit: vi.fn(), commerceResubmit: vi.fn(),
    commerceRefresh: vi.fn(),
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('../content/AssetPicker', () => ({
  AssetPicker: ({ accept, onPick, onClose }: { accept: string[]; onPick: (assets: { id: string; kind: 'image' | 'text'; path: string; mime: string; text: string | null; title: string; origin: null }[]) => void; onClose: () => void }) => (
    <div>
      <button type="button" onClick={() => {
        if (accept.includes('text')) onPick([{ id: 'text-1', kind: 'text', path: '/tmp/copy.md', mime: 'text/markdown', text: '档案描述', title: '档案标题', origin: null }])
        else onPick([{ id: 'img-1', kind: 'image', path: '/tmp/from-asset.png', mime: 'image/png', text: null, title: '主图', origin: null }])
        onClose()
      }}>确认产物</button>
    </div>
  ),
}))

function shop(partial: Partial<ShopConnection> = {}): ShopConnection {
  return {
    id: 's1', platform: 'shopee', remoteId: 'r1', name: '虾皮一店', region: 'SG',
    boundAt: '2026-10-01T00:00:00Z', checkedAt: '2026-10-02T00:00:00Z', status: 'connected', detail: null,
    ...partial,
  }
}

function caps(listing: boolean, platform: CommerceCapabilities['platform'] = 'shopee'): CommerceCapabilities {
  return { platform, metrics: [], listing, categories: listing, products: listing, orders: listing, notes: listing ? [] : ['该平台尚未接入上架'] }
}

function record(partial: Partial<ListingRecord> = {}): ListingRecord {
  return {
    id: 'rec-1', groupId: 'g1', shopId: 's1', platform: 'shopee', status: 'failed', reason: '图片不合格',
    remoteId: '900', title: '帆布鞋', currency: 'SGD',
    draft: {
      title: '帆布鞋', description: '档案描述', price: 19.9, currency: 'SGD', stock: 4,
      skus: [{ name: 'SKU-1', code: 'SKU-1', price: 19.9, stock: 4 }],
      images: ['/tmp/shoe.png'], weightKg: 0.4, dimensionsCm: { l: 30, w: 20, h: 10 },
    },
    target: { shopId: 's1', categoryId: '100', attributes: [{ id: '9', value: '帆布', values: [] }] },
    attempts: 1, createdAt: '2026-10-02T00:00:00Z', updatedAt: '2026-10-02T00:00:00Z',
    ...partial,
  }
}

const archive: Product = {
  id: 'p1', revision: 1, name: '档案鞋', description: '不要用我', currency: 'USD', price: 1, stock: 1,
  skus: [], images: [], createdAt: '2026-10-02T00:00:00Z', updatedAt: '2026-10-02T00:00:00Z',
}

beforeEach(() => {
  vi.mocked(api.commerceShops).mockReset().mockResolvedValue([])
  vi.mocked(api.productsList).mockReset().mockResolvedValue([archive])
  vi.mocked(api.commerceCapabilities).mockReset().mockResolvedValue(caps(true))
  vi.mocked(api.commerceCategories).mockReset().mockResolvedValue([])
  vi.mocked(api.commerceAttributes).mockReset().mockResolvedValue([{ id: '9', name: '材质', required: true, input: 'text', options: [] }])
  vi.mocked(api.commerceListings).mockReset().mockResolvedValue([])
  vi.mocked(api.commerceSubmit).mockReset().mockResolvedValue([])
  vi.mocked(api.commerceResubmit).mockReset().mockImplementation(async (id) => record({ id, status: 'reviewing' }))
  vi.mocked(api.commerceRefresh).mockReset().mockImplementation(async (id) => record({ id, status: 'live', reason: undefined, title: '不确定鞋' }))
  vi.mocked(open).mockReset().mockResolvedValue('/tmp/shoe.png')
})

async function chooseLeaf(shopName: string, categoryName: string) {
  fireEvent.click(screen.getByRole('button', { name: `类目 ${shopName}` }))
  fireEvent.click(await screen.findByRole('option', { name: categoryName }))
}

it('assembles one record per shop from files and assets without requiring an archive', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([
    shop(),
    shop({ id: 's2', name: '虾皮二店', remoteId: 'r2' }),
    shop({ id: 's3', name: '希音店', platform: 'shein', remoteId: 'r3' }),
  ])
  vi.mocked(api.commerceCapabilities).mockImplementation(async (id: string) => caps(id !== 's3', id === 's3' ? 'shein' : 'shopee'))
  vi.mocked(api.commerceCategories).mockImplementation(async (id: string) => [{
    id: id === 's1' ? '100' : '200', name: id === 's1' ? '鞋' : '包', parentId: '0', leaf: true,
  }])
  vi.mocked(api.commerceSubmit).mockImplementation(async (draft, targets) => targets.map((target, index) => record({
    id: `rec-${index}`, shopId: target.shopId, title: draft.title, draft, target, status: 'reviewing', reason: undefined,
  })))
  render(<ListingPage />)
  await waitFor(() => expect(screen.getByRole('button', { name: '虾皮一店' })).toBeEnabled())
  expect(screen.getByRole('button', { name: '希音店' })).toBeDisabled()

  fireEvent.click(screen.getByRole('button', { name: '从产物库选择文案' }))
  fireEvent.click(screen.getByRole('button', { name: '确认产物' }))
  fireEvent.change(screen.getByRole('textbox', { name: '标题' }), { target: { value: '帆布鞋' } })
  fireEvent.change(screen.getByRole('textbox', { name: '价格' }), { target: { value: '19.9' } })
  fireEvent.change(screen.getByRole('textbox', { name: '币种' }), { target: { value: 'sgd' } })
  fireEvent.change(screen.getByRole('textbox', { name: '库存' }), { target: { value: '4' } })
  fireEvent.change(screen.getByRole('textbox', { name: 'SKU' }), { target: { value: 'SKU-1' } })
  fireEvent.change(screen.getByRole('textbox', { name: '重量（千克）' }), { target: { value: '0.4' } })
  fireEvent.change(screen.getByRole('textbox', { name: '长' }), { target: { value: '30' } })
  fireEvent.change(screen.getByRole('textbox', { name: '宽' }), { target: { value: '20' } })
  fireEvent.change(screen.getByRole('textbox', { name: '高' }), { target: { value: '10' } })
  fireEvent.click(screen.getByRole('button', { name: '从文件选择' }))
  expect(await screen.findByText('shoe.png')).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '从产物库选择图片' }))
  fireEvent.click(screen.getByRole('button', { name: '确认产物' }))
  expect(await screen.findByText('from-asset.png')).toBeInTheDocument()

  fireEvent.click(screen.getByRole('button', { name: '虾皮一店' }))
  fireEvent.click(screen.getByRole('button', { name: '虾皮二店' }))
  await chooseLeaf('虾皮一店', '鞋')
  await chooseLeaf('虾皮二店', '包')
  fireEvent.change(await screen.findByRole('textbox', { name: '材质 虾皮一店' }), { target: { value: '帆布' } })
  fireEvent.change(screen.getByRole('textbox', { name: '材质 虾皮二店' }), { target: { value: '尼龙' } })
  fireEvent.click(screen.getByRole('button', { name: '提交上架' }))

  await waitFor(() => expect(api.commerceSubmit).toHaveBeenCalledTimes(1))
  const [draft, targets, groupId] = vi.mocked(api.commerceSubmit).mock.calls[0]
  expect(groupId).toBeNull()
  expect(draft).toMatchObject({
    title: '帆布鞋', description: '档案描述', currency: 'SGD', price: 19.9, stock: 4, weightKg: 0.4,
    images: ['/tmp/shoe.png', '/tmp/from-asset.png'],
  })
  expect(draft).not.toHaveProperty('productId')
  expect(targets.map((target) => target.shopId)).toEqual(['s1', 's2'])
  expect(targets.map((target) => target.categoryId)).toEqual(['100', '200'])
  expect(await screen.findAllByRole('cell', { name: '审核中' })).toHaveLength(2)
})

it('shows validation and submit failures, and retries only a failed record', async () => {
  render(<ListingPage />)
  fireEvent.click(await screen.findByRole('button', { name: '提交上架' }))
  expect(await screen.findByText(/商品标题需为 1 到 255 个字符/)).toBeInTheDocument()
  expect(api.commerceSubmit).not.toHaveBeenCalled()

  vi.mocked(api.commerceListings).mockResolvedValue([
    record({ id: 'bad', title: '失败鞋', status: 'failed', reason: '图片不合格' }),
    record({ id: 'ok', title: '已上架鞋', status: 'live', reason: undefined }),
    record({ id: 'unsure', title: '不确定鞋', status: 'uncertain', reason: '平台超时' }),
  ])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('cell', { name: '图片不合格' })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: '重新提交 已上架鞋' })).toBeNull()
  expect(screen.queryByRole('button', { name: '重新提交 不确定鞋' })).toBeNull()
  vi.mocked(api.commerceResubmit).mockRejectedValueOnce(new Error('提交结果不确定，只能查询，不能重新提交'))
  fireEvent.click(screen.getByRole('button', { name: '重新提交 失败鞋' }))
  expect(await screen.findByText('提交结果不确定，只能查询，不能重新提交')).toBeInTheDocument()
  expect(screen.getByRole('cell', { name: '失败' })).toBeInTheDocument()

  vi.mocked(api.commerceRefresh).mockResolvedValue(record({ id: 'unsure', title: '不确定鞋', status: 'live', reason: undefined }))
  fireEvent.click(screen.getByRole('button', { name: '刷新状态 不确定鞋' }))
  await waitFor(() => expect(api.commerceRefresh).toHaveBeenCalledWith('unsure'))
  expect(screen.queryByRole('button', { name: '重新提交 不确定鞋' })).toBeNull()
})

it('edits a failed record and resubmits that record instead of creating one', async () => {
  vi.mocked(api.commerceShops).mockResolvedValue([shop()])
  vi.mocked(api.commerceListings).mockResolvedValue([record()])
  render(<ListingPage />)
  await waitFor(() => expect(screen.getByRole('button', { name: '虾皮一店' })).toBeEnabled())
  fireEvent.click(screen.getByRole('button', { name: '修改 帆布鞋' }))
  expect(await screen.findByRole('textbox', { name: '材质 虾皮一店' })).toHaveValue('帆布')
  fireEvent.change(screen.getByRole('textbox', { name: '标题' }), { target: { value: '新标题' } })
  fireEvent.click(screen.getByRole('button', { name: '提交上架' }))
  await waitFor(() => expect(api.commerceResubmit).toHaveBeenCalledWith(
    'rec-1',
    expect.objectContaining({ title: '新标题' }),
    expect.objectContaining({ shopId: 's1', categoryId: '100' }),
  ))
  expect(api.commerceSubmit).not.toHaveBeenCalled()
})

it('drops a late category page and a late shop list', async () => {
  let resolveChild: (value: Category[]) => void = () => {}
  vi.mocked(api.commerceShops).mockResolvedValue([shop()])
  vi.mocked(api.commerceCategories).mockImplementation((_id: string, parentId?: string | null) => {
    if (parentId === '9') return new Promise((resolve) => { resolveChild = resolve })
    return Promise.resolve([{ id: '9', name: '服装', parentId: '0', leaf: false }])
  })
  render(<ListingPage />)
  await waitFor(() => expect(screen.getByRole('button', { name: '虾皮一店' })).toBeEnabled())
  fireEvent.click(screen.getByRole('button', { name: '虾皮一店' }))
  fireEvent.click(screen.getByRole('button', { name: '类目 虾皮一店' }))
  fireEvent.click(await screen.findByRole('option', { name: '服装 /' }))
  fireEvent.click(await screen.findByRole('button', { name: '返回上级 虾皮一店' }))
  fireEvent.click(screen.getByRole('button', { name: '类目 虾皮一店' }))
  expect(await screen.findByRole('option', { name: '服装 /' })).toBeInTheDocument()
  await act(async () => { resolveChild([{ id: '2', name: '鞋子', parentId: '9', leaf: true }]) })
  expect(screen.queryByRole('option', { name: '鞋子' })).toBeNull()
  expect(screen.getByRole('option', { name: '服装 /' })).toBeInTheDocument()

  let resolveShops: (value: ShopConnection[]) => void = () => {}
  vi.mocked(api.commerceShops).mockImplementationOnce(() => new Promise((resolve) => { resolveShops = resolve }))
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  await waitFor(() => expect(api.commerceShops).toHaveBeenCalledTimes(2))
  vi.mocked(api.commerceShops).mockResolvedValueOnce([shop({ id: 's2', name: '虾皮二店' })])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('button', { name: '虾皮二店' })).toBeInTheDocument()
  await act(async () => { resolveShops([shop({ name: '旧店' })]) })
  expect(screen.queryByRole('button', { name: '旧店' })).toBeNull()
})

it('loads listings again after reopen and ignores the previous visit', async () => {
  let resolveList: (value: ListingRecord[]) => void = () => {}
  vi.mocked(api.commerceListings).mockImplementationOnce(() => new Promise((resolve) => { resolveList = resolve }))
  const view = render(<ListingPage />)
  await waitFor(() => expect(api.commerceListings).toHaveBeenCalledTimes(1))
  view.unmount()
  vi.mocked(api.commerceListings).mockResolvedValue([record({ title: '新记录', status: 'live', reason: undefined })])
  render(<ListingPage />)
  expect(await screen.findByRole('cell', { name: '新记录' })).toBeInTheDocument()
  await act(async () => { resolveList([record({ title: '旧记录' })]) })
  expect(screen.queryByRole('cell', { name: '旧记录' })).toBeNull()
  expect(screen.getByRole('cell', { name: '新记录' })).toBeInTheDocument()
})
