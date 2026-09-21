export type ListingPlatformId = 'douyin' | 'kuaishou' | 'wechat'
export type ListingCheckLevel = 'error' | 'warn' | 'ok'

export interface ListingCheckItem {
  id: string
  level: ListingCheckLevel
  /** 填进 i18n 模板的数字 / 词。 */
  params?: Record<string, string | number>
}

export interface ListingCheckInput {
  platform: ListingPlatformId
  title: string
  sellingPoints: string
  description: string
}

export interface ListingCheckResult {
  items: ListingCheckItem[]
  titleLength: number
  titleMax: number
  titleMin: number
}

/** 广告法常见极限词。各平台审核都会打。 */
const AD_LIMIT_WORDS = ['最好', '第一', '唯一', '顶级', '国家级', '世界级', '万能', '100%', '史上', '绝对'] as const

/** 微信小店《商品信息发布规范》点名的空泛标题词。 */
const WECHAT_VAGUE_WORDS = ['好物', '孤品', '秒杀', '百货', '链接', '按编号'] as const

const DECORATIVE = /[★☆●◆■√✔]/
const URL = /https?:\/\/|\bwww\./i
const CONTACT = /微信|v信|\bwx\b|qq[:：]|电话[:：]|1[3-9]\d{9}/i

const PLATFORM_TITLE: Record<ListingPlatformId, { min: number; max: number }> = {
  /** 卖家 SOP / 行业常用口径：抖店标题上限 60。 */
  douyin: { min: 5, max: 60 },
  /** 快手公开规则没写死字数，按同行常用 60 做软上限。 */
  kuaishou: { min: 5, max: 60 },
  /** 微信开放文档：title 最多 60 字符；规范要求至少 5 个汉字。 */
  wechat: { min: 5, max: 60 },
}

export function countChars(text: string): number {
  return [...text.trim()].length
}

export function countHan(text: string): number {
  return (text.match(/[\u4e00-\u9fff]/g) ?? []).length
}

function hits(text: string, words: readonly string[]): string[] {
  return words.filter((word) => text.includes(word))
}

function repeatedTokens(title: string): string[] {
  const tokens = title.match(/[\u4e00-\u9fff]{2,}/g) ?? []
  const counts = new Map<string, number>()
  for (const token of tokens) counts.set(token, (counts.get(token) ?? 0) + 1)
  return [...counts.entries()].filter(([, n]) => n > 2).map(([token]) => token)
}

/**
 * 上架文案检查。只跑本地规则，不请求模型、不编造商品。
 * 字数口径：抖店/微信按公开上限 60；微信汉字下限来自《商品信息发布规范》。
 */
export function checkListing(input: ListingCheckInput): ListingCheckResult {
  const title = input.title.trim()
  const body = `${input.sellingPoints}\n${input.description}`
  const all = `${title}\n${body}`
  const titleLength = countChars(title)
  const { min: titleMin, max: titleMax } = PLATFORM_TITLE[input.platform]
  const items: ListingCheckItem[] = []

  if (!title) {
    items.push({ id: 'titleEmpty', level: 'error' })
  } else {
    if (titleLength > titleMax) {
      items.push({ id: 'titleTooLong', level: 'error', params: { n: titleLength, max: titleMax } })
    } else if (titleLength > titleMax - 10) {
      items.push({ id: 'titleNearLimit', level: 'warn', params: { n: titleLength, max: titleMax } })
    } else {
      items.push({ id: 'titleLengthOk', level: 'ok', params: { n: titleLength, max: titleMax } })
    }

    const han = countHan(title)
    if (input.platform === 'wechat' && han < titleMin) {
      items.push({ id: 'titleHanMin', level: 'error', params: { n: han, min: titleMin } })
    }

    if (/^[\dA-Za-z\s]+$/.test(title)) {
      items.push({ id: 'titleLatinOnly', level: 'error' })
    }

    const repeats = repeatedTokens(title)
    if (repeats.length > 0) {
      items.push({ id: 'titleRepeat', level: 'warn', params: { words: repeats.join('、') } })
    }

    if (DECORATIVE.test(title)) {
      items.push({ id: 'titleDecor', level: 'warn' })
    }

    if (input.platform === 'wechat') {
      const vague = hits(title, WECHAT_VAGUE_WORDS)
      if (vague.length > 0) {
        items.push({ id: 'titleVague', level: 'error', params: { words: vague.join('、') } })
      }
    }
  }

  const ad = hits(all, AD_LIMIT_WORDS)
  if (ad.length > 0) {
    items.push({ id: 'adWords', level: 'error', params: { words: ad.join('、') } })
  } else {
    items.push({ id: 'adWordsOk', level: 'ok' })
  }

  if (URL.test(all) || CONTACT.test(all)) {
    items.push({ id: 'contactLeak', level: 'error' })
  } else {
    items.push({ id: 'contactOk', level: 'ok' })
  }

  return { items, titleLength, titleMax, titleMin }
}
