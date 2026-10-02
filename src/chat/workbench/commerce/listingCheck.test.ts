import { describe, expect, it } from 'vitest'
import { buildListingAdviceRequest, checkListing, countChars, countHan, countWeighted } from './listingCheck'

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

  it('uses each platform title limit', () => {
    expect(checkListing({ platform: 'shopee', title: 'Hat', sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooShort')).toBe(true)
    expect(checkListing({ platform: 'shopee', title: 'a'.repeat(101), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong' && item.params?.max === 100)).toBe(true)
    expect(checkListing({ platform: 'shein', title: 'a'.repeat(256), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong' && item.params?.max === 255)).toBe(true)
    expect(checkListing({ platform: 'tiktok', title: 'a'.repeat(24), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooShort' && item.params?.min === 25)).toBe(true)
    expect(checkListing({ platform: 'tiktok', title: 'a'.repeat(201), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong' && item.params?.max === 200)).toBe(true)
    expect(checkListing({ platform: 'mercadolibre', title: 'a'.repeat(61), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong' && item.params?.max === 60)).toBe(true)
    const taobao = '裤'.repeat(31)
    expect(countWeighted(taobao)).toBe(62)
    expect(checkListing({ platform: 'taobao', title: taobao, sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong')).toBe(true)
    expect(checkListing({ platform: 'pinduoduo', title: '裤'.repeat(30), sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleTooLong')).toBe(false)
    expect(checkListing({ platform: 'shopee', title: '纯棉短袖夏季', sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleLanguageEn')).toBe(true)
    expect(checkListing({ platform: 'mercadolibre', title: '纯棉短袖夏季', sellingPoints: '', description: '' }).items.some((item) => item.id === 'titleLanguageEs')).toBe(true)
    expect(checkListing({ platform: 'shopee', title: 'Cotton tee', sellingPoints: '', description: 'Free Shipping' }).items.some((item) => item.id === 'promoWords')).toBe(true)
    expect(checkListing({ platform: 'tiktok', title: 'Cotton short sleeve shirt daily', sellingPoints: '', description: 'whatsapp me' }).items.some((item) => item.id === 'contactLeak')).toBe(true)
  })

  it('checks optional images and skips them when none are provided', () => {
    const bare = checkListing({ platform: 'douyin', title: '纯棉抗菌男士短袖T恤 夏季宽松白色', sellingPoints: '', description: '' })
    expect(bare.items.some((item) => item.id.startsWith('image'))).toBe(false)
    const tooMany = checkListing({
      platform: 'douyin',
      title: '纯棉抗菌男士短袖T恤 夏季宽松白色',
      sellingPoints: '',
      description: '',
      images: Array.from({ length: 10 }, () => ({ width: 800, height: 800, bytes: 1000 })),
    })
    expect(tooMany.items.some((item) => item.id === 'imageTooMany')).toBe(true)
    const bad = checkListing({
      platform: 'taobao',
      title: '纯棉抗菌男士短袖T恤 夏季宽松白色',
      sellingPoints: '',
      description: '',
      images: [{ name: 'cover.jpg', width: 300, height: 400, bytes: 4 * 1024 * 1024 }],
    })
    expect(bad.items.some((item) => item.id === 'imageTooSmall')).toBe(true)
    expect(bad.items.some((item) => item.id === 'imageRatio')).toBe(true)
    expect(bad.items.some((item) => item.id === 'imageTooLarge')).toBe(true)
    const shopee = checkListing({
      platform: 'shopee',
      title: 'Cotton tee',
      sellingPoints: '',
      description: '',
      images: [{ width: 800, height: 800, bytes: 1000 }, { width: 500, height: 800, bytes: 1000 }],
    })
    expect(shopee.items.some((item) => item.id === 'imageTooFew')).toBe(true)
    expect(shopee.items.some((item) => item.id === 'imageRatio' && item.level === 'warn')).toBe(true)
    const meli = checkListing({
      platform: 'mercadolibre',
      title: 'Camiseta algodon',
      sellingPoints: '',
      description: '',
      images: [{ width: 800, height: 800, bytes: 1000 }],
    })
    expect(meli.items.some((item) => item.id === 'imageTooSmall' && item.params?.min === 1200)).toBe(true)
    const ok = checkListing({
      platform: 'pinduoduo',
      title: '纯棉抗菌男士短袖T恤 夏季宽松白色',
      sellingPoints: '',
      description: '',
      images: [{ width: 600, height: 600, bytes: 200_000 }],
    })
    expect(ok.items.some((item) => item.id === 'imageOk')).toBe(true)
  })

  it('builds an advice prompt from the listing and the rule hits', () => {
    const input = { platform: 'shopee' as const, title: 'Hat', sellingPoints: 'cotton', description: '' }
    const result = checkListing(input)
    const advice = buildListingAdviceRequest(input, result)
    expect(advice.prompt).toContain('shopee')
    expect(advice.prompt).toContain('Hat')
    expect(advice.prompt).toContain('titleTooShort')
    expect(advice.prompt).toContain('Do not apply')
  })
})
