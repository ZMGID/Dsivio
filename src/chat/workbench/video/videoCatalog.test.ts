import { describe, expect, it } from 'vitest'
import { DRAMA_STYLES, VIDEO_TASK_TABS } from './videoCatalog'

describe('videoCatalog', () => {
  it('keeps the live drama style set and task tabs', () => {
    expect(DRAMA_STYLES.map((item) => item.id)).toEqual([
      'twist', 'romance', 'comedy', 'mystery', 'workplace',
      'family', 'ceo', 'rebirth', 'schemer',
    ])
    expect(VIDEO_TASK_TABS.map((item) => item.id)).toEqual(['all', 'queued', 'running', 'done', 'failed'])
  })
})
