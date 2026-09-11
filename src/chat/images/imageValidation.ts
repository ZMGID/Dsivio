import type { ImageBrief, ImageConfig } from './types'
export function imageConfigIssue(config: ImageConfig, brief: ImageBrief): string {
  if (!config.providerId || !config.model) return '请先在图片设置中选择图片供应商和生成模型'
  if (!['openai', 'grok', 'gemini', 'gemini-chat', 'async'].includes(config.protocol)) return '请选择图片接口协议'
  if (!['1:1', '2:3', '3:2', '3:4', '4:3', '4:5', '5:4', '9:16', '16:9'].includes(brief.ratio)) return '请选择支持的画幅'
  if (!['1k', '2k', '4k'].includes(brief.resolution)) return '请选择生成清晰度'
  if (config.protocol === 'openai' && (!['1:1', '2:3', '3:2'].includes(brief.ratio) || brief.resolution !== '1k')) return 'OpenAI 标准接口需要 1:1、2:3 或 3:2 画幅及 1K 清晰度'
  if (config.protocol === 'grok' && (brief.resolution === '4k' || ['4:5', '5:4'].includes(brief.ratio))) return 'Grok 不支持 4K、4:5 或 5:4，请调整生成规格'
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
  if (lower.includes('upstream request failed') || /\b502\b/.test(text)) {
    return '上游生图暂时失败，请稍后单独重试该页，不要整单重提。'
  }
  if (lower.includes('16-multiple') || lower.includes('image-2 size') || (lower.includes('invalid_request') && lower.includes('size'))) {
    return '该模型需要像素尺寸（如 1024x1024），不能把画幅比例直接当作 size。请重新生成。'
  }
  if (text.startsWith('图片接口 HTTP') || text.startsWith('图片接口拒绝')) return text
  return text
}
