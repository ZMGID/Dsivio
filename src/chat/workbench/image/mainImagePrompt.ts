import type { I18n } from '../../../components/i18n'
import { IMAGE_STYLES, sizeForImageRatio, type ImageRatioId, type ImageStyleId } from './imageCatalog'

export interface MainImageBrief {
  brief: string
  style: ImageStyleId
  ratio: ImageRatioId
}

/**
 * 主图提示词：用户描述在前，风格与出图规格在后。
 * 一个页面只有这一处决定「提交给模型的文本长什么样」。
 */
export function buildMainImagePrompt(input: MainImageBrief, t: I18n): string {
  const style = IMAGE_STYLES.find((item) => item.id === input.style)
  const lines = [
    input.brief.trim(),
    `${t.workbenchMainStyle}: ${style ? t[style.label] : input.style}`,
    `${t.workbenchImageRatio}: ${input.ratio} (${sizeForImageRatio(input.ratio)})`,
    t.workbenchMainPromptTail,
  ]
  return lines.filter(Boolean).join('\n')
}
