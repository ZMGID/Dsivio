import { expect, it } from 'vitest'
import { subAgentStatusLabel } from './subAgentStatus'

it('shows execution facts and ignores legacy acceptance judgments', () => {
  expect(subAgentStatusLabel({ status: 'completed', requiresReview: true }, 'zh')).toBe('已返回')
  expect(subAgentStatusLabel({ status: 'failed', error: 'recovered: Report' }, 'zh')).toBe('已返回')
  expect(subAgentStatusLabel({ status: 'failed', requiresReview: true }, 'zh')).toBe('执行已结束')
  expect(subAgentStatusLabel({ status: 'failed', resolution: { outcome: 'completed_by_parent' } }, 'zh')).toBe('执行已结束')
  expect(subAgentStatusLabel({ status: 'completed', resolution: { outcome: 'blocked' } }, 'zh')).toBe('已返回')
  expect(subAgentStatusLabel({ status: 'stopping', resolution: { outcome: 'blocked' } }, 'zh')).toBe('正在停止')
})
