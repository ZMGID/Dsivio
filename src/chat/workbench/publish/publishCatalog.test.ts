import { describe, expect, it } from 'vitest'
import { ACCOUNT_KPIS, PUBLISH_DESC_MAX, PUBLISH_LOG_TABS, PUBLISH_TITLE_MAX } from './publishCatalog'

describe('publishCatalog', () => {
  it('keeps the live title, desc, log, and account slots', () => {
    expect(PUBLISH_TITLE_MAX).toBe(30)
    expect(PUBLISH_DESC_MAX).toBe(200)
    expect(PUBLISH_LOG_TABS.map((item) => item.id)).toEqual(['all', 'queued', 'done'])
    expect(ACCOUNT_KPIS.map((item) => item.id)).toEqual(['bound', 'ok', 'bad', 'fans'])
  })
})
