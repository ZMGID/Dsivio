import { IMAGE_RATIOS, IMAGE_RESOLUTIONS, isAllowedImageOutput } from './imageOutput'
import type { ImageBrief, ImageConfig } from './types'
export function imageConfigIssue(config: ImageConfig, brief: ImageBrief): string {
  if (!config.providerId || !config.model) return '请先在图片设置中选择图片供应商和生成模型'
  if (!['openai', 'grok', 'gemini', 'gemini-chat', 'async'].includes(config.protocol)) return '请选择图片接口协议'
  if (!(IMAGE_RATIOS as readonly string[]).includes(brief.ratio) || !(IMAGE_RESOLUTIONS as readonly string[]).includes(brief.resolution)) {
    return '请选择分辨率'
  }
  if (!isAllowedImageOutput(brief.ratio, brief.resolution, config.model, config.protocol)) {
    if (config.model.toLowerCase().includes('gpt-image-2')) return 'gpt-image-2 请使用官方尺寸：1:1、2:3、3:2、9:16、16:9'
    if (config.protocol === 'openai') return 'OpenAI 标准接口只支持 1024×1024、1024×1536 或 1536×1024'
    if (config.protocol === 'grok') return 'Grok 不支持 4K，只支持 1K/2K 以及 1:1、16:9、9:16、4:3、3:4、3:2、2:3'
    return '请选择当前模型支持的分辨率'
  }
  if (!Number.isInteger(brief.count) || brief.count < 1 || brief.count > 30) return '每套图片数量须为 1–30 的整数'
  if (brief.products.length > 200 || brief.products.some(p => p.assets.filter(a => !a.name.startsWith('__dsimage_')).length > 16)) return '每个任务最多 200 款商品，每款最多 16 张参考图'
  return ''
}

/** Turn gateway dumps (400 size JSON, Cloudflare 504) into a short retry hint. */
export function humanizeImageError(error: string | null | undefined): string {
  if (!error) return ''
  const text = error
    .replace(/\\u003c/gi, '<')
    .replace(/\\u003e/gi, '>')
    .replace(/\\u003d/gi, '=')
  const lower = text.toLowerCase()
  if (lower.includes('gateway timeout') || lower.includes('error code: 504') || /\b504\b/.test(text)) {
    return '图片生成超时，兼容网关无法同步等待出图。请重新生成。'
  }
  if (/\b502\b/.test(text) && !lower.includes('upstream request failed')) {
    return '上游生图暂时失败，请稍后单独重试该页，不要整单重提。'
  }
  if (lower.includes('16-multiple') || lower.includes('image-2 size') || (lower.includes('invalid_request') && lower.includes('size'))) {
    return '该模型需要像素尺寸（如 1024x1024），不能把画幅比例直接当作 size。请重新生成。'
  }
  if (text.startsWith('图片接口 HTTP') || text.startsWith('图片接口拒绝')) return text
  return text
}
