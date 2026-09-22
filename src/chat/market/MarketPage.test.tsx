import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import type { MarketSnapshot } from './types'
import { MarketPage } from './MarketPage'
import { marketApi } from './api'

const state = vi.hoisted(() => ({ snapshot: {} as MarketSnapshot, loading: false, initialLoading: false, discard: vi.fn() }))
vi.mock('./api', () => ({ useMarket: () => ({ snapshot: state.snapshot, loading: state.loading, initialLoading: state.initialLoading }), marketApi: { refresh: vi.fn(), icon: vi.fn().mockResolvedValue(''), discard: state.discard } }))
beforeEach(() => {
  window.location.hash = '#chat/market'
  state.snapshot = { categories: [], entries: [], installed: [], refreshedAt: null, error: null, sourceUrl: '' }
  state.discard.mockReset().mockResolvedValue(undefined)
})
afterEach(cleanup)
const show = () => render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={vi.fn()} />)

it('空目录显示暂无插件，不显示故障或重试', () => {
  show()
  expect(screen.getByText('暂无插件')).toBeTruthy()
  expect(screen.queryByText('暂时无法读取市场。')).toBeNull()
  expect(screen.queryByText('重试')).toBeNull()
})
it('浏览器预览是正常空状态，安装限制单独说明', () => {
  state.snapshot.preview = true
  show()
  expect(screen.getByText('暂无插件')).toBeTruthy()
  expect(screen.getByText(/浏览器预览/)).toBeTruthy()
  expect(screen.queryByText('重试')).toBeNull()
})
it('真实加载失败仍显示重试，不冒充空目录', () => {
  state.snapshot.error = '网络连接失败'
  show()
  expect(screen.getByText('市场暂不可用')).toBeTruthy()
  expect(screen.getByText('重试')).toBeTruthy()
  expect(screen.queryByText('暂无插件')).toBeNull()
})

it('后台刷新不替换已有空状态', () => {
  state.loading = true
  const { container } = show()
  expect(screen.getByText('暂无插件')).toBeTruthy()
  expect(container.querySelector('.market-skeleton')).toBeNull()
  expect(screen.getByRole('button', {name: '刷新'}).hasAttribute('disabled')).toBe(true)
  state.loading = false
})

it('本机试用文档包可以进入安装流程', async () => {
  const manifest = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  state.snapshot.entries = [{ id: 'srt-whiteboard-animation', version: '0.1.0', source: { kind: 'local-draft', directory: '/trial/srt-whiteboard-animation', revision: 'a'.repeat(64) }, manifest: { ...manifest, schemaVersion: 1 } }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  expect(screen.getByText(manifest.name)).toBeTruthy()
  fireEvent.click(screen.getByRole('button', {name: '安装'}))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('srt-whiteboard-animation'))
})

it('卸载直接交给对话，不弹出本地删除确认', async () => {
  const manifest = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  state.snapshot.installed = [{ id: 'srt-whiteboard-animation', manifest: { ...manifest, schemaVersion: 1 }, source: { kind: 'local-draft', directory: '/trial/srt-whiteboard-animation', revision: 'a'.repeat(64) }, status: 'ready', enabled: true, pluginId: 'test', skillId: 'test', conversationId: null, error: null }]
  const onUninstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  fireEvent.click(screen.getByRole('button', {name: `${manifest.name} 更多操作`}))
  fireEvent.click(screen.getByRole('button', {name: '卸载'}))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledWith('srt-whiteboard-animation'))
  expect(screen.queryByRole('alertdialog')).toBeNull()
})

it('未完成的市场插件留在公开页，卸载直接清掉安装记录', async () => {
  const manifest = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  state.snapshot.installed = [{ id: 'srt-whiteboard-animation', manifest: { ...manifest, schemaVersion: 1 }, source: { kind: 'local-draft', directory: '/trial/srt-whiteboard-animation', revision: 'a'.repeat(64) }, status: 'configuring', enabled: false, pluginId: null, skillId: null, conversationId: null, error: null }]
  const onUninstall = vi.fn()
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  expect(screen.getByRole('tab', { name: '公开', selected: true })).toBeTruthy()
  expect(screen.getByRole('button', { name: '继续安装' })).toBeTruthy()
  fireEvent.click(screen.getByRole('tab', { name: '个人' }))
  expect(screen.queryByRole('button', { name: '继续安装' })).toBeNull()
  fireEvent.click(screen.getByRole('tab', { name: '公开' }))
  fireEvent.click(screen.getByRole('button', { name: `${manifest.name} 更多操作` }))
  fireEvent.click(screen.getByRole('button', { name: '卸载' }))
  await waitFor(() => expect(state.discard).toHaveBeenCalledWith('srt-whiteboard-animation'))
  expect(onUninstall).not.toHaveBeenCalled()
})

it('内置 Hypit 按真实安装状态显示，已安装时提供卸载入口', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, id: 'hypit', name: 'Hypit 视频制作', icon: 'assets/hypit-logo.svg', schemaVersion: 1 as const }
  const iconUrl = `data:image/svg+xml;base64,${btoa('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"/>')}`
  vi.mocked(marketApi.icon).mockResolvedValue(iconUrl)
  const entry = { id: 'hypit', version: manifest.version, source: { kind: 'built-in' as const }, manifest }
  state.snapshot.entries = [entry]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  const { container, unmount } = render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  await waitFor(() => expect(container.querySelector('.market-icon-hypit img')?.getAttribute('src')).toBe(iconUrl))
  expect(screen.getByText('未安装')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '安装' }))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('hypit'))
  unmount()

  state.snapshot.installed = [{ id: 'hypit', manifest, source: { kind: 'built-in' }, status: 'ready', enabled: true, pluginId: null, skillId: 'hypit', conversationId: null, error: null }]
  const onUninstall = vi.fn().mockResolvedValue(undefined)
  const installedView = render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  await waitFor(() => expect(installedView.container.querySelectorAll('.market-icon-hypit img').length).toBe(2))
  expect(screen.getAllByText('已安装').length).toBeGreaterThan(1)
  expect(screen.getByRole('button', { name: 'Hypit 视频制作' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: 'Hypit 视频制作 更多操作' }))
  fireEvent.click(screen.getByRole('button', { name: '卸载' }))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledWith('hypit'))
  installedView.unmount()
  state.snapshot.installed = [{ ...state.snapshot.installed[0], phase: 'removing' }]
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  expect(screen.getByText('卸载待完成')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '继续卸载' }))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledTimes(2))
})

it('Remotion Agent Skills 在视频分类显示安装与已安装状态', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, id: 'remotion-agent-skills', name: 'Remotion Agent Skills', icon: 'assets/remotion-logo.svg', schemaVersion: 1 as const }
  state.snapshot.categories = [{ id: 'videos', name: '视频' }]
  state.snapshot.entries = [{ id: manifest.id, version: manifest.version, source: { kind: 'built-in' }, manifest }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  const first = render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  expect(screen.getByText('Remotion Agent Skills')).toBeTruthy()
  expect(screen.getByText('未安装')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '安装' }))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('remotion-agent-skills'))
  first.unmount()

  state.snapshot.installed = [{ id: manifest.id, manifest, source: { kind: 'built-in' }, status: 'ready', enabled: true, pluginId: null, skillId: 'remotion-best-practices', conversationId: null, error: null }]
  const onUse = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={onUse} onUninstall={vi.fn()} />)
  expect(screen.getAllByText('已安装').length).toBeGreaterThan(1)
  fireEvent.click(screen.getByRole('button', { name: '使用' }))
  await waitFor(() => expect(onUse).toHaveBeenCalled())
})

it('SRT 白板手绘动画作为内置视频插件提供安装和卸载入口', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, name: 'SRT 白板手绘动画', icon: 'assets/srt-whiteboard-logo.svg', schemaVersion: 1 as const }
  state.snapshot.categories = [{ id: 'videos', name: '视频' }]
  state.snapshot.entries = [{ id: manifest.id, version: manifest.version, source: { kind: 'built-in' }, manifest }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  const first = render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  expect(screen.getByText('SRT 白板手绘动画')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '安装' }))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('srt-whiteboard-animation'))
  first.unmount()

  state.snapshot.installed = [{ id: manifest.id, manifest, source: { kind: 'built-in' }, status: 'ready', enabled: true, pluginId: null, skillId: manifest.id, conversationId: null, error: null }]
  const onUninstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={vi.fn()} onUse={vi.fn()} onUninstall={onUninstall} />)
  expect(screen.getByRole('button', { name: 'SRT 白板手绘动画' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: 'SRT 白板手绘动画 更多操作' }))
  fireEvent.click(screen.getByRole('button', { name: '卸载' }))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledWith('srt-whiteboard-animation'))
})

it('内置插件详情展示 setup、主 Skill 与环境检查命令', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, schemaVersion: 1 as const, setupSkillId: 'srt-whiteboard-animation-setup', mainSkillId: 'srt-whiteboard-animation', skillIds: ['srt-whiteboard-animation'], checkCommand: '/srt-whiteboard-market:check' }
  state.snapshot.entries = [{ id: manifest.id, version: manifest.version, source: { kind: 'built-in' }, manifest }]
  window.location.hash = '#chat/market/srt-whiteboard-animation'
  show()
  expect(screen.getByText('srt-whiteboard-animation-setup')).toBeTruthy()
  expect(screen.getByText('srt-whiteboard-animation')).toBeTruthy()
  expect(screen.getByText('/srt-whiteboard-market:check')).toBeTruthy()
})

it('没有命令包的内置组件丢失时仍可修复或卸载', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, schemaVersion: 1 as const, id: 'feishu-cli', name: '飞书 CLI', checkCommand: null }
  state.snapshot.entries = [{ id: manifest.id, version: manifest.version, source: { kind: 'built-in' }, manifest }]
  state.snapshot.installed = [{ id: manifest.id, manifest, source: { kind: 'built-in' }, status: 'failed', enabled: false, pluginId: null, skillId: null, conversationId: null, error: '插件组件缺失' }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  const onUninstall = vi.fn().mockResolvedValue(undefined)
  render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={onUninstall} />)
  expect(screen.getByText('需修复')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '重新配置' }))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('feishu-cli'))
  fireEvent.click(screen.getByRole('button', { name: '飞书 CLI 更多操作' }))
  fireEvent.click(screen.getByRole('button', { name: '卸载' }))
  await waitFor(() => expect(onUninstall).toHaveBeenCalledWith('feishu-cli'))
  expect(state.discard).not.toHaveBeenCalled()
})

it('飞书 CLI 在效率办公分类只展示两个 Skill，不展示命令', async () => {
  const base = (await import('../../../packages/srt-whiteboard-animation/market.json')).default.manifest
  const manifest = { ...base, schemaVersion: 1 as const, id: 'feishu-cli', name: '飞书 CLI', categoryIds: ['productivity'], setupSkillId: 'feishu-cli-setup', mainSkillId: 'feishu-cli', skillIds: ['feishu-cli'], checkCommand: null }
  state.snapshot.categories = [{ id: 'productivity', name: '效率办公' }]
  state.snapshot.entries = [{ id: manifest.id, version: manifest.version, source: { kind: 'built-in' }, manifest }]
  const onInstall = vi.fn().mockResolvedValue(undefined)
  const view = render(<MarketPage lang="zh" onInstall={onInstall} onUse={vi.fn()} onUninstall={vi.fn()} />)
  expect(screen.getByText('效率办公')).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: '安装' }))
  await waitFor(() => expect(onInstall).toHaveBeenCalledWith('feishu-cli'))
  view.unmount()
  window.location.hash = '#chat/market/feishu-cli'
  show()
  expect(screen.getByText('feishu-cli-setup')).toBeTruthy()
  expect(screen.getByText('feishu-cli')).toBeTruthy()
  expect(screen.queryByText('命令')).toBeNull()
})
