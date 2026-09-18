import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { MarketSnapshot } from './types'
import { MarketPage } from './MarketPage'

const state = vi.hoisted(() => ({ snapshot: {} as MarketSnapshot, loading: false, initialLoading: false }))
vi.mock('./api', () => ({ useMarket: () => ({ snapshot: state.snapshot, loading: state.loading, initialLoading: state.initialLoading }), marketApi: { refresh: vi.fn(), icon: vi.fn().mockResolvedValue('') } }))
beforeEach(() => {
  window.location.hash = '#chat/market'
  state.snapshot = { categories: [], entries: [], installed: [], refreshedAt: null, error: null, sourceUrl: '' }
})
afterEach(cleanup)
const show = () => render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={vi.fn()} />)

it('空目录显示暂无应用，不显示故障或重试', () => {
  show()
  expect(screen.getByText('暂无应用')).toBeTruthy()
  expect(screen.queryByText('暂时无法读取市场。')).toBeNull()
  expect(screen.queryByText('重试')).toBeNull()
})
it('浏览器预览是正常空状态，安装限制单独说明', () => {
  state.snapshot.preview = true
  show()
  expect(screen.getByText('暂无应用')).toBeTruthy()
  expect(screen.getByText(/浏览器预览/)).toBeTruthy()
  expect(screen.queryByText('重试')).toBeNull()
})
it('真实加载失败仍显示重试，不冒充空目录', () => {
  state.snapshot.error = '网络连接失败'
  show()
  expect(screen.getByText('市场暂不可用')).toBeTruthy()
  expect(screen.getByText('重试')).toBeTruthy()
  expect(screen.queryByText('暂无应用')).toBeNull()
})

it('后台刷新不替换已有空状态', () => {
  state.loading = true
  const { container } = show()
  expect(screen.getByText('暂无应用')).toBeTruthy()
  expect(container.querySelector('.market-skeleton')).toBeNull()
  expect(screen.getByRole('button', {name: '刷新'}).hasAttribute('disabled')).toBe(true)
  state.loading = false
})

it('本机试用文档包可以进入安装流程', async () => {
  const manifest = (await import('../../../packages/hypit/market.json')).default.manifest
  state.snapshot.entries = [{ id: 'hypit', version: '0.1.0', source: { kind: 'local-draft', directory: '/trial/hypit', revision: 'a'.repeat(64) }, manifest: { ...manifest, schemaVersion: 1 } }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  expect(screen.getByText(manifest.name)).toBeTruthy()
  fireEvent.click(screen.getByRole('button', {name: '安装'}))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('hypit'))
})

it('卸载直接交给对话，不弹出本地删除确认', async () => {
  const manifest = (await import('../../../packages/hypit/market.json')).default.manifest
  state.snapshot.installed = [{ id: 'hypit', manifest: { ...manifest, schemaVersion: 1 }, source: { kind: 'local-draft', directory: '/trial/hypit', revision: 'a'.repeat(64) }, status: 'ready', enabled: true, pluginId: 'test', skillId: 'test', conversationId: null, error: null }]
  const onUninstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  fireEvent.click(screen.getByRole('button', {name: `${manifest.name} 更多操作`}))
  fireEvent.click(screen.getByRole('button', {name: '卸载'}))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledWith('hypit'))
  expect(screen.queryByRole('alertdialog')).toBeNull()
})
