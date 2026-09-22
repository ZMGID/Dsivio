import { describe, expect, it } from 'vitest'
import { filterWorkbenchFeatures, type WorkbenchFeature } from './workbenchFeatures'

const features: WorkbenchFeature[] = [
  { page: 'home', label: '首页', group: '' },
  { page: 'shops', label: '店铺绑定', group: '电商自动化' },
  { page: 'match', label: '同款找货', group: '智能选品' },
]

describe('filterWorkbenchFeatures', () => {
  it('empty query keeps the catalog', () => {
    expect(filterWorkbenchFeatures(features, '  ')).toEqual(features)
  })

  it('matches a feature name', () => {
    expect(filterWorkbenchFeatures(features, '店铺')).toEqual([features[1]])
  })

  it('matches a group name', () => {
    expect(filterWorkbenchFeatures(features, '选品')).toEqual([features[2]])
  })
})
