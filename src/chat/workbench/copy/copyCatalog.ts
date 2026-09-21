import type { I18n } from '../../../components/i18n'

export type PostTemplateId = 'xhs' | 'douyin' | 'kuaishou' | 'taobao' | 'channels' | 'moments'
export type PostRatioId = '1:1' | '3:4' | '4:3' | '9:16' | '16:9'
export type ArticlePlatformId = 'wechat' | 'xhs' | 'zhihu' | 'private'
export type ArticleTypeId =
  | 'seed' | 'review' | 'list' | 'scene' | 'compare' | 'unbox'
  | 'guide' | 'pitfall' | 'gift' | 'launch' | 'season' | 'repeat'
export type ArticleLengthId = 'short' | 'standard' | 'long'

export const POST_TEMPLATES: { id: PostTemplateId; name: keyof I18n; desc: keyof I18n }[] = [
  { id: 'xhs', name: 'workbenchPostTplXhs', desc: 'workbenchPostTplXhsDesc' },
  { id: 'douyin', name: 'workbenchPostTplDouyin', desc: 'workbenchPostTplDouyinDesc' },
  { id: 'kuaishou', name: 'workbenchPostTplKuaishou', desc: 'workbenchPostTplKuaishouDesc' },
  { id: 'taobao', name: 'workbenchPostTplTaobao', desc: 'workbenchPostTplTaobaoDesc' },
  { id: 'channels', name: 'workbenchPostTplChannels', desc: 'workbenchPostTplChannelsDesc' },
  { id: 'moments', name: 'workbenchPostTplMoments', desc: 'workbenchPostTplMomentsDesc' },
]

export const POST_RATIOS: { id: PostRatioId; label: keyof I18n }[] = [
  { id: '1:1', label: 'workbenchPostRatio11' },
  { id: '3:4', label: 'workbenchPostRatio34' },
  { id: '4:3', label: 'workbenchPostRatio43' },
  { id: '9:16', label: 'workbenchPostRatio916' },
  { id: '16:9', label: 'workbenchPostRatio169' },
]

export const ARTICLE_PLATFORMS: { id: ArticlePlatformId; name: keyof I18n; desc: keyof I18n }[] = [
  { id: 'wechat', name: 'workbenchArticlePlatWechat', desc: 'workbenchArticlePlatWechatDesc' },
  { id: 'xhs', name: 'workbenchArticlePlatXhs', desc: 'workbenchArticlePlatXhsDesc' },
  { id: 'zhihu', name: 'workbenchArticlePlatZhihu', desc: 'workbenchArticlePlatZhihuDesc' },
  { id: 'private', name: 'workbenchArticlePlatPrivate', desc: 'workbenchArticlePlatPrivateDesc' },
]

export const ARTICLE_TYPES: { id: ArticleTypeId; label: keyof I18n }[] = [
  { id: 'seed', label: 'workbenchArticleTypeSeed' },
  { id: 'review', label: 'workbenchArticleTypeReview' },
  { id: 'list', label: 'workbenchArticleTypeList' },
  { id: 'scene', label: 'workbenchArticleTypeScene' },
  { id: 'compare', label: 'workbenchArticleTypeCompare' },
  { id: 'unbox', label: 'workbenchArticleTypeUnbox' },
  { id: 'guide', label: 'workbenchArticleTypeGuide' },
  { id: 'pitfall', label: 'workbenchArticleTypePitfall' },
  { id: 'gift', label: 'workbenchArticleTypeGift' },
  { id: 'launch', label: 'workbenchArticleTypeLaunch' },
  { id: 'season', label: 'workbenchArticleTypeSeason' },
  { id: 'repeat', label: 'workbenchArticleTypeRepeat' },
]

export const ARTICLE_LENGTHS: { id: ArticleLengthId; label: keyof I18n; capsule: keyof I18n }[] = [
  { id: 'short', label: 'workbenchArticleLenShort', capsule: 'workbenchArticleLenShortCap' },
  { id: 'standard', label: 'workbenchArticleLenStandard', capsule: 'workbenchArticleLenStandardCap' },
  { id: 'long', label: 'workbenchArticleLenLong', capsule: 'workbenchArticleLenLongCap' },
]

/** 3:4 活页是 1920x2560，其余按同一长边推。 */
export function sizeForRatio(ratio: PostRatioId): string {
  switch (ratio) {
    case '1:1':
      return '1920x1920'
    case '3:4':
      return '1920x2560'
    case '4:3':
      return '2560x1920'
    case '9:16':
      return '1440x2560'
    case '16:9':
      return '2560x1440'
  }
}
