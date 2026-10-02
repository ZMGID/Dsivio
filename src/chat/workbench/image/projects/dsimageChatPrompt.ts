import type { ImageBrief, ImageFeature, ImageProduct } from './types'

const SET_FEATURES = ['replace', 'smart', 'design', 'client'] as const

export type ImageSetChatFeature = (typeof SET_FEATURES)[number]

export function isImageSetChatFeature(feature: ImageFeature): feature is ImageSetChatFeature {
  return (SET_FEATURES as readonly string[]).includes(feature)
}

const ROUTE: Record<ImageSetChatFeature, string> = {
  replace: 'replace（图片复刻：沿用样图版式，只换商品）',
  smart: 'smart（模板套图：沿用所选模板的风格，按这个商品写内容）',
  design: 'design（套图设计：没有模板，按要求从零设计一套）',
  client: 'client（批量套图：这一批商品按品类分套，每类先出样品）',
}

function productLines(product: ImageProduct): string[] {
  const lines: string[] = []
  const seen = new Set<string>()
  const push = (label: string, path: string | null | undefined) => {
    const value = path?.trim()
    if (!value || seen.has(value)) return
    seen.add(value)
    lines.push(label ? `${label} ${value}` : value)
  }
  push('正面', product.front)
  push('背面', product.back)
  for (const asset of product.assets) push('', asset.path)
  return lines
}

/** 新对话草稿。首行 `/dsimage` 会在发送时挂上 dsimage 技能。 */
export function dsimageChatPrompt(brief: ImageBrief): string {
  if (!isImageSetChatFeature(brief.feature)) throw new Error('这个页面没有「用对话做」')
  const lines = [
    '/dsimage',
    `请用 dsimage 技能，走 ${ROUTE[brief.feature]}。`,
  ]
  if (brief.name.trim()) lines.push(`任务：${brief.name.trim()}`)
  lines.push(brief.requirement.trim() ? `要求：${brief.requirement.trim()}` : '要求：（工作台里还没写）')
  if (brief.language.trim()) lines.push(`图内文字：${brief.language.trim()}`)
  if (brief.style.trim()) lines.push(`风格：${brief.style.trim()}`)
  if (brief.platform.trim()) lines.push(`平台：${brief.platform.trim()}`)
  if (brief.templateId) lines.push(`模板：${brief.templateId}`)
  const sources = brief.workflowInput?.sources ?? []
  if (sources.length) lines.push(`样图：${sources.map((asset) => asset.path).join('，')}`)
  lines.push(`画幅 ${brief.ratio}，分辨率 ${brief.resolution}，每款 ${brief.count} 张。`)
  if (!brief.products.length) {
    lines.push('商品图：还没有。')
  } else {
    lines.push('商品图：')
    for (const product of brief.products) {
      const paths = productLines(product)
      const title = product.name.trim() || '未命名'
      const category = product.category.trim()
      const head = category ? `${title}（${category}）` : title
      lines.push(`- ${head}：${paths.length ? paths.join('，') : '没有图片路径'}`)
      if (product.facts.trim()) lines.push(`  已知信息：${product.facts.trim()}`)
    }
  }
  return lines.join('\n')
}
