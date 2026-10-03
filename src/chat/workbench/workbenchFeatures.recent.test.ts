import { describe, expect, it } from 'vitest'
import type { MediaTask } from '../../generated/mediaGeneration'
import { recentWorkbenchTasks, taskSourcePage } from './workbenchFeatures'

function task(id: string, origin: string | null, createdAt: string): MediaTask {
  return { id, providerId: 'p', model: 'm', kind: 'image', status: 'succeeded', createdAt, error: null, remoteId: null, outputs: [], canResume: false, origin, prompt: '', result: null, requestHash: null, cancellation: null }
}

describe('taskSourcePage', () => {
  it('maps workbench origins to a registered page', () => {
    expect(taskSourcePage('workbench/main')).toBe('main')
  })

  it('rejects chat origins and unknown pages', () => {
    expect(taskSourcePage('chat/studio')).toBeNull()
    expect(taskSourcePage('workbench/gone')).toBeNull()
    expect(taskSourcePage(null)).toBeNull()
  })
})

describe('recentWorkbenchTasks', () => {
  it('keeps only workbench tasks, newest first, capped', () => {
    const tasks = [
      task('a', 'workbench/main', '2026-10-01T00:00:00Z'),
      task('b', 'chat/studio', '2026-10-03T00:00:00Z'),
      task('c', 'workbench/shorts', '2026-10-02T00:00:00Z'),
      task('d', 'workbench/main', '2026-10-04T00:00:00Z'),
    ]
    expect(recentWorkbenchTasks(tasks, 2).map((item) => item.id)).toEqual(['d', 'c'])
  })
})
