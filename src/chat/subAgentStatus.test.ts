import { expect, it } from 'vitest'
import { subAgentStatusLabel, subAgentNeedsAttention } from './subAgentStatus'

it('separates returned output, parent acceptance and legacy recovery', () => {
  expect(subAgentStatusLabel({ status: 'completed', requiresReview: true }, 'zh')).toBe('等待主代理验收')
  expect(subAgentStatusLabel({ status: 'failed', error: 'recovered: Report' }, 'zh')).toBe('有恢复结果')
  expect(subAgentStatusLabel({ status: 'failed', requiresReview: true }, 'zh')).toBe('等待主代理处理')
  expect(subAgentStatusLabel({ status: 'failed', resolution: { outcome: 'completed_by_parent' } }, 'zh')).toBe('主代理已补齐')
  expect(subAgentNeedsAttention({ status: 'completed', resolution: { outcome: 'blocked' } })).toBe(true)
  expect(subAgentStatusLabel({ status: 'stopping', resolution: { outcome: 'blocked' } }, 'zh')).toBe('正在停止')
})
