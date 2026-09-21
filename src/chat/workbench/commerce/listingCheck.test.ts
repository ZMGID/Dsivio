import { describe, expect, it } from 'vitest'
import { checkListing, countChars, countHan } from './listingCheck'

describe('checkListing', () => {
  it('flags an empty title', () => {
    const result = checkListing({ platform: 'douyin', title: '', sellingPoints: '', description: '' })
    expect(result.items.some((item) => item.id === 'titleEmpty' && item.level === 'error')).toBe(true)
  })

  it('uses the WeChat 60-character cap and 5-han minimum from the official spec', () => {
    const short = checkListing({ platform: 'wechat', title: '帽子', sellingPoints: '', description: '' })
    expect(countHan('帽子')).toBe(2)
    expect(short.items.some((item) => item.id === 'titleHanMin' && item.level === 'error')).toBe(true)

    const long = '糖醋排骨'.repeat(16)
    expect(countChars(long)).toBeGreaterThan(60)
    const overflow = checkListing({ platform: 'wechat', title: long, sellingPoints: '', description: '' })
    expect(overflow.items.some((item) => item.id === 'titleTooLong')).toBe(true)
  })

  it('rejects WeChat vague title words named in the store rules', () => {
    const result = checkListing({
      platform: 'wechat',
      title: '直播好物分享按编号下单',
      sellingPoints: '',
      description: '',
    })
    const hit = result.items.find((item) => item.id === 'titleVague')
    expect(hit?.level).toBe('error')
    expect(String(hit?.params?.words)).toContain('好物')
  })

  it('catches advertising-law superlatives and outbound contact', () => {
    const result = checkListing({
      platform: 'kuaishou',
      title: '纯棉短袖T恤 夏季宽松',
      sellingPoints: '全国第一 最好穿',
      description: '加微信 13800138000',
    })
    expect(result.items.some((item) => item.id === 'adWords' && item.level === 'error')).toBe(true)
    expect(result.items.some((item) => item.id === 'contactLeak' && item.level === 'error')).toBe(true)
  })

  it('passes a plain compliant title', () => {
    const result = checkListing({
      platform: 'douyin',
      title: '纯棉抗菌男士短袖T恤 夏季宽松白色',
      sellingPoints: '200g 纯棉 圆领',
      description: '日常休闲上衣',
    })
    expect(result.items.every((item) => item.level !== 'error')).toBe(true)
    expect(result.items.some((item) => item.id === 'titleLengthOk')).toBe(true)
  })
})
