export type ListingPlatformId =
  | 'douyin'
  | 'kuaishou'
  | 'wechat'
  | 'taobao'
  | 'pinduoduo'
  | 'shopee'
  | 'shein'
  | 'tiktok'
  | 'mercadolibre'

export type ListingCheckLevel = 'error' | 'warn' | 'ok'

export interface ListingCheckItem {
  id: string
  level: ListingCheckLevel
  /** 填进 i18n 模板的数字 / 词。 */
  params?: Record<string, string | number>
}

export interface ListingImageInput {
  name?: string
  width?: number
  height?: number
  bytes?: number
}

export interface ListingCheckInput {
  platform: ListingPlatformId
  title: string
  sellingPoints: string
  description: string
  /** 不传或空数组时跳过图片规则。 */
  images?: ListingImageInput[]
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

/** Shopee / TikTok 卖家教育点名的促销词。 */
const OVERSEAS_PROMO = ['best seller', 'free shipping', 'hot item', '#1'] as const

const DECORATIVE = /[★☆●◆■√✔]/
const URL = /https?:\/\/|\bwww\./i
const CONTACT = /微信|v信|\bwx\b|qq[:：]|电话[:：]|1[3-9]\d{9}|whatsapp|telegram|t\.me\/|\bwechat\b/i

const MB = 1024 * 1024

type TitleSpec = { min: number; max: number; unit: 'chars' | 'weighted'; hanMin?: number; latinOnly?: boolean; vague?: boolean }
type ImageSpec = {
  min: number
  max: number
  minEdge: number
  maxBytes: number
  ratio: number | null
  ratioLevel: 'error' | 'warn'
}

/**
 * 字数来源（硬上限，按各平台公开卖家/开放文档）：
 * - 抖店 / 快手：标题上限 60。快手未写死字数，沿用 60 软上限。
 * - 微信小店：title 最多 60；规范至少 5 个汉字。
 * - 淘宝 / 拼多多：标题 60 字符，汉字计 2、英文数字计 1（约 30 个汉字）。
 * - Shopee Open Platform v2 product.get_item_limit：item_name_length_limit 默认 min 5 / max 100
 *   （店铺接口可能更紧）。卖家教育另要求主图至少 500px、建议不少于 3 张。
 * - SHEIN 卖家刊登标题上限 255。
 * - TikTok Shop Seller University（US，Product Listing）：标题 25–200；方图最多 9 张、至少 600px。
 * - Mercado Libre：标题最多 60（max_title_length）；图片长边建议至少 1200，张数按类目，这里用常见上限 12。
 */
const PLATFORM_TITLE: Record<ListingPlatformId, TitleSpec> = {
  douyin: { min: 5, max: 60, unit: 'chars', latinOnly: true },
  kuaishou: { min: 5, max: 60, unit: 'chars', latinOnly: true },
  wechat: { min: 5, max: 60, unit: 'chars', hanMin: 5, latinOnly: true, vague: true },
  taobao: { min: 1, max: 60, unit: 'weighted', latinOnly: true },
  pinduoduo: { min: 1, max: 60, unit: 'weighted', latinOnly: true },
  shopee: { min: 5, max: 100, unit: 'chars' },
  shein: { min: 1, max: 255, unit: 'chars' },
  tiktok: { min: 25, max: 200, unit: 'chars' },
  mercadolibre: { min: 1, max: 60, unit: 'chars' },
}

const PLATFORM_IMAGE: Record<ListingPlatformId, ImageSpec> = {
  douyin: { min: 1, max: 9, minEdge: 800, maxBytes: 3 * MB, ratio: 1, ratioLevel: 'error' },
  kuaishou: { min: 1, max: 9, minEdge: 800, maxBytes: 3 * MB, ratio: 1, ratioLevel: 'warn' },
  wechat: { min: 1, max: 9, minEdge: 800, maxBytes: 2 * MB, ratio: 1, ratioLevel: 'error' },
  taobao: { min: 1, max: 5, minEdge: 800, maxBytes: 3 * MB, ratio: 1, ratioLevel: 'error' },
  pinduoduo: { min: 1, max: 10, minEdge: 480, maxBytes: 1 * MB, ratio: 1, ratioLevel: 'error' },
  shopee: { min: 3, max: 9, minEdge: 500, maxBytes: 2 * MB, ratio: 1, ratioLevel: 'warn' },
  shein: { min: 1, max: 11, minEdge: 800, maxBytes: 3 * MB, ratio: 1, ratioLevel: 'error' },
  tiktok: { min: 5, max: 9, minEdge: 600, maxBytes: 5 * MB, ratio: 1, ratioLevel: 'error' },
  mercadolibre: { min: 1, max: 12, minEdge: 1200, maxBytes: 10 * MB, ratio: null, ratioLevel: 'warn' },
}

const OVERSEAS_EN = new Set<ListingPlatformId>(['shopee', 'shein', 'tiktok'])

export function countChars(text: string): number {
  return [...text.trim()].length
}

export function countHan(text: string): number {
  return (text.match(/[\u4e00-\u9fff]/g) ?? []).length
}

/** 淘宝 / 拼多多：汉字 2，其余 1。 */
export function countWeighted(text: string): number {
  let total = 0
  for (const ch of text.trim()) total += /[\u4e00-\u9fff]/.test(ch) ? 2 : 1
  return total
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

function checkImages(platform: ListingPlatformId, images: ListingImageInput[] | undefined, items: ListingCheckItem[]) {
  if (!images || images.length === 0) return
  const spec = PLATFORM_IMAGE[platform]
  let problem = false
  if (images.length > spec.max) {
    problem = true
    items.push({ id: 'imageTooMany', level: 'error', params: { n: images.length, max: spec.max } })
  } else if (images.length < spec.min) {
    problem = true
    items.push({ id: 'imageTooFew', level: 'warn', params: { n: images.length, min: spec.min } })
  }
  images.forEach((image, index) => {
    const name = image.name?.trim() || String(index + 1)
    if (image.bytes != null && spec.maxBytes > 0 && image.bytes > spec.maxBytes) {
      problem = true
      items.push({ id: 'imageTooLarge', level: 'error', params: { name, maxMb: spec.maxBytes / MB } })
    }
    if (image.width && image.height) {
      const edge = Math.max(image.width, image.height)
      if (spec.minEdge && edge < spec.minEdge) {
        problem = true
        items.push({ id: 'imageTooSmall', level: 'error', params: { name, edge, min: spec.minEdge } })
      }
      if (spec.ratio && Math.abs(image.width / image.height - spec.ratio) > 0.02) {
        problem = true
        items.push({ id: 'imageRatio', level: spec.ratioLevel, params: { name, ratio: '1:1' } })
      }
    }
  })
  if (!problem) items.push({ id: 'imageOk', level: 'ok' })
}

/**
 * 上架文案检查。只跑本地规则，不请求模型、不编造商品。
 * 图片规则仅在传入了至少一张图时运行。
 */
export function checkListing(input: ListingCheckInput): ListingCheckResult {
  const title = input.title.trim()
  const body = `${input.sellingPoints}\n${input.description}`
  const all = `${title}\n${body}`
  const spec = PLATFORM_TITLE[input.platform]
  const titleLength = spec.unit === 'weighted' ? countWeighted(title) : countChars(title)
  const items: ListingCheckItem[] = []

  if (!title) {
    items.push({ id: 'titleEmpty', level: 'error' })
  } else {
    if (titleLength > spec.max) {
      items.push({ id: 'titleTooLong', level: 'error', params: { n: titleLength, max: spec.max } })
    } else if (titleLength < spec.min) {
      items.push({ id: 'titleTooShort', level: 'error', params: { n: titleLength, min: spec.min } })
    } else if (titleLength > spec.max - 10) {
      items.push({ id: 'titleNearLimit', level: 'warn', params: { n: titleLength, max: spec.max } })
    } else {
      items.push({ id: 'titleLengthOk', level: 'ok', params: { n: titleLength, max: spec.max } })
    }

    const han = countHan(title)
    if (spec.hanMin != null && han < spec.hanMin) {
      items.push({ id: 'titleHanMin', level: 'error', params: { n: han, min: spec.hanMin } })
    }

    if (spec.latinOnly && /^[\dA-Za-z\s]+$/.test(title)) {
      items.push({ id: 'titleLatinOnly', level: 'error' })
    }

    const repeats = repeatedTokens(title)
    if (repeats.length > 0) {
      items.push({ id: 'titleRepeat', level: 'warn', params: { words: repeats.join('、') } })
    }

    if (DECORATIVE.test(title)) {
      items.push({ id: 'titleDecor', level: 'warn' })
    }

    if (spec.vague) {
      const vague = hits(title, WECHAT_VAGUE_WORDS)
      if (vague.length > 0) {
        items.push({ id: 'titleVague', level: 'error', params: { words: vague.join('、') } })
      }
    }

    const latin = (title.match(/[A-Za-z]/g) ?? []).length
    if (han > latin) {
      if (input.platform === 'mercadolibre') items.push({ id: 'titleLanguageEs', level: 'warn' })
      else if (OVERSEAS_EN.has(input.platform)) items.push({ id: 'titleLanguageEn', level: 'warn' })
    }
  }

  const ad = hits(all, AD_LIMIT_WORDS)
  if (ad.length > 0) {
    items.push({ id: 'adWords', level: 'error', params: { words: ad.join('、') } })
  } else {
    items.push({ id: 'adWordsOk', level: 'ok' })
  }

  if (OVERSEAS_EN.has(input.platform) || input.platform === 'mercadolibre') {
    const lower = all.toLowerCase()
    const promo = OVERSEAS_PROMO.filter((word) => lower.includes(word))
    if (promo.length > 0) items.push({ id: 'promoWords', level: 'error', params: { words: promo.join(', ') } })
  }

  if (URL.test(all) || CONTACT.test(all)) {
    items.push({ id: 'contactLeak', level: 'error' })
  } else {
    items.push({ id: 'contactOk', level: 'ok' })
  }

  checkImages(input.platform, input.images, items)
  return { items, titleLength, titleMax: spec.max, titleMin: spec.min }
}

/** Prompt for the suggestion button. The page shows the reply and does not write it back. */
export function buildListingAdviceRequest(input: ListingCheckInput, result: ListingCheckResult): { system: string; prompt: string } {
  const issues = result.items
    .filter((item) => item.level !== 'ok')
    .map((item) => `${item.level} ${item.id} ${JSON.stringify(item.params ?? {})}`)
    .join('\n')
  return {
    system: 'You suggest marketplace listing edits. Do not invent certifications, prices, materials, or claims that are not in the draft. Reply in the draft language. These are suggestions only; do not tell the user you changed the listing.',
    prompt: [
      `Platform: ${input.platform}`,
      `Title: ${input.title}`,
      `Selling points: ${input.sellingPoints}`,
      `Description: ${input.description}`,
      `Images provided: ${input.images?.length ?? 0}`,
      `Title length ${result.titleLength} (min ${result.titleMin}, max ${result.titleMax})`,
      'Rule hits:',
      issues || 'none',
      'Give concrete edit suggestions. Do not apply them.',
    ].join('\n'),
  }
}
