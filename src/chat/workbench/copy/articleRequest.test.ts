import { expect, it } from 'vitest'
import { articleTitle, buildArticleRequest } from './articleRequest'

it('assembles one markdown request from product info, platform, type, and length', () => {
  const request = buildArticleRequest({
    brief: '通勤帆布包，容量大',
    platform: 'xhs',
    type: 'review',
    length: 'long',
    imageCount: 2,
  })
  expect(request.system).toContain('Markdown')
  expect(request.prompt).toContain('通勤帆布包，容量大')
  expect(request.prompt).toContain('发布平台：小红书长文（xhs）')
  expect(request.prompt).toContain('文章类型：测评体验（review）')
  expect(request.prompt).toContain('篇幅：1100-1600字')
  expect(request.prompt).toContain('参考图数量：2')
  expect(request.prompt.trim().length).toBeGreaterThan(0)
})

it('still produces a prompt when the only input is images', () => {
  const request = buildArticleRequest({ brief: '  ', platform: 'wechat', type: 'seed', length: 'short', imageCount: 1 })
  expect(request.prompt).toContain('参考图数量：1')
  expect(request.prompt).toContain('没有文字说明')
  expect(request.prompt).toContain('500-700字')
})

it('names the record from the heading, then the brief', () => {
  expect(articleTitle('# 通勤包\n正文', '别的')).toBe('通勤包')
  expect(articleTitle('没有标题\n正文', '帆布包容量大')).toBe('帆布包容量大')
  expect(articleTitle('', '')).toBe('种草文章')
  expect(articleTitle(`# ${'很'.repeat(100)}`, '短')).toHaveLength(80)
})
