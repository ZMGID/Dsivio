import { describe, expect, it } from 'vitest'
import { marketItems, marketUsePrompt, primaryAction, type MarketLocal, type MarketSnapshot } from './types'

const local = { id: 'test', manifest: { name: '已安装版本' }, status: 'ready', enabled: true } as MarketLocal
const snapshot: MarketSnapshot = { categories: [], entries: [], installed: [local], refreshedAt: null, error: '离线', sourceUrl: '' }

describe('应用市场本地状态', () => {
  it('离线或下架后仍保留已安装应用及使用入口', () => {
    const items = marketItems(snapshot)
    expect(items).toHaveLength(1)
    expect(items[0].manifest?.name).toBe('已安装版本')
    expect(primaryAction(items[0])).toBe('use')
  })
  it('关闭应用后必须先加载，安装未完成不能使用', () => {
    expect(primaryAction({ id: 'test', local: { ...local, enabled: false } })).toBe('enable-use')
    expect(primaryAction({ id: 'test', local: { ...local, status: 'configuring' } })).toBe('continue')
    expect(primaryAction({ id: 'test' })).toBe('unavailable')
  })
  it('远程清单失败时仍保留本地说明和使用入口', () => {
    const items = marketItems({ ...snapshot, entries: [{ id: 'test', version: '2.0.0', source: {} as never, error: '网络失败' }] })
    expect(items).toHaveLength(1)
    expect(items[0].manifest).toBe(local.manifest)
    expect(primaryAction(items[0])).toBe('use')
  })
  it('发布新版不会让旧版应用卡片展示未安装的说明', () => {
    const items = marketItems({ ...snapshot, entries: [{ id: 'test', version: '2.0.0', source: {} as never, manifest: { name: '未安装新版' } as never }] })
    expect(items[0].manifest).toBe(local.manifest)
    expect(items[0].entry?.version).toBe('2.0.0')
  })
})

it('本机试用文档变更后提供重新配置，正式发布版本保持旧版可用', () => {
  const installed = { ...local, source: { kind: 'local-draft' as const, directory: '/draft', revision: 'old' } }
  expect(primaryAction({ id: 'test', local: installed, entry: { id: 'test', version: '0.1.1', source: { kind: 'local-draft', directory: '/draft', revision: 'new' } } })).toBe('repair')
  expect(primaryAction({ id: 'test', local: installed, entry: { id: 'test', version: '0.1.0', source: installed.source } })).toBe('use')
})

it('使用已安装插件时发送明确启动请求，不把使用示例当作用户消息', () => {
  const withPrompt = { ...local, source: { kind: 'built-in' }, manifest: { ...local.manifest, startPrompt: '  使用飞书 CLI，告诉我可以做什么。  ', inputHint: '帮我删除一份文档' } } as MarketLocal
  expect(marketUsePrompt(withPrompt)).toBe('使用飞书 CLI，告诉我可以做什么。')
  expect(marketUsePrompt({ ...withPrompt, source: { repository: 'example/repo', revision: 'abc', directory: '/example' } })).toBe('使用这个插件，告诉我可以做什么。')
  const withoutPrompt = { ...local, source: { kind: 'built-in' }, manifest: { ...local.manifest, inputHint: '帮我删除一份文档' } } as MarketLocal
  expect(marketUsePrompt(withoutPrompt)).toBe('使用这个插件，告诉我可以做什么。')
  expect(marketUsePrompt(withoutPrompt)).not.toContain('删除一份文档')
})
