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
