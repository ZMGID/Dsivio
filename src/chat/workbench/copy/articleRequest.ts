import type { ArticleLengthId, ArticlePlatformId, ArticleTypeId } from './copyCatalog'

const PLATFORM: Record<ArticlePlatformId, string> = {
  wechat: '公众号推文',
  xhs: '小红书长文',
  zhihu: '知乎好物',
  private: '私域社群',
}

const TYPE: Record<ArticleTypeId, string> = {
  seed: '种草推荐',
  review: '测评体验',
  list: '清单合集',
  scene: '场景解决方案',
  compare: '对比推荐',
  unbox: '开箱体验',
  guide: '使用教程',
  pitfall: '避坑指南',
  gift: '送礼推荐',
  launch: '新品上新',
  season: '季节场景',
  repeat: '复购推荐',
}

const LENGTH: Record<ArticleLengthId, string> = {
  short: '500-700字',
  standard: '800-1100字',
  long: '1100-1600字',
}

export type ArticleDraft = {
  brief: string
  platform: ArticlePlatformId
  type: ArticleTypeId
  length: ArticleLengthId
  imageCount: number
}

/** One `useAiTask` `once` call. The page does not add a second prompt shape. */
export function buildArticleRequest(draft: ArticleDraft): { system: string; prompt: string } {
  const brief = draft.brief.trim()
  return {
    system: [
      '你是电商种草作者。根据产品信息和参考图写一篇可直接发布的长文。',
      '只输出 Markdown：第一行是一级标题，正文用小标题分段。不要前言，不要解释写法。',
      '不要编造产品信息里没有的认证、价格、材质或功效。',
    ].join('\n'),
    prompt: [
      `发布平台：${PLATFORM[draft.platform]}（${draft.platform}）`,
      `文章类型：${TYPE[draft.type]}（${draft.type}）`,
      `篇幅：${LENGTH[draft.length]}`,
      `参考图数量：${draft.imageCount}`,
      '产品信息：',
      brief || '（没有文字说明，只根据参考图写；图里看不出的参数不要编造）',
    ].join('\n'),
  }
}

/** Title stored on the media record. Prefer the markdown heading. */
export function articleTitle(markdown: string, fallback: string): string {
  const heading = markdown.split('\n').map((line) => line.trim()).find((line) => line.startsWith('# '))
  const fromHeading = heading?.replace(/^#\s+/, '').trim() ?? ''
  const fromFallback = fallback.trim().split('\n')[0]?.trim() ?? ''
  return (fromHeading || fromFallback || '种草文章').slice(0, 80)
}
