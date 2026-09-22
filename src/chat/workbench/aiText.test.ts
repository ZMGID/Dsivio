import { expect, it } from 'vitest'
import { stripFences } from './aiText'

it('strips a code fence', () => {
  expect(stripFences('```\n把这段代码按模块拆开\n```')).toBe('把这段代码按模块拆开')
})

it('strips a leading label', () => {
  expect(stripFences('优化后的问题：明天北京下雨吗？')).toBe('明天北京下雨吗？')
})

it('returns empty for blank input', () => {
  expect(stripFences('   ')).toBe('')
})
