import { describe, expect, it } from 'vitest'
import { emptyBrief, suggestImageTaskName } from './types'

describe('suggestImageTaskName', () => {
  it('uses the feature label and language market when the name is left blank', () => {
    expect(suggestImageTaskName(emptyBrief('workflow'))).toBe('制作模板 · 中国市场')
    expect(
      suggestImageTaskName({
        ...emptyBrief('gen'),
        language: 'en-US',
      }),
    ).toBe('单张 / 改图 · 美国市场')
  })
  it('prefers a real product name over the feature label', () => {
    expect(
      suggestImageTaskName({
        ...emptyBrief('workflow'),
        products: [
          {
            id: 'a',
            name: '通勤双肩包',
            facts: '',
            kind: '',
            category: '背包',
            front: null,
            back: null,
            templateId: null,
            assets: [],
          },
        ],
      }),
    ).toBe('通勤双肩包 · 中国市场')
  })
  it('drops the market when the image has no text', () => {
    expect(suggestImageTaskName({ ...emptyBrief('gen'), language: '无文字' })).toBe('单张 / 改图')
  })
})
