import { videoRatios, type VideoBrief, type VideoBootstrap } from './types'

// Keep these gates aligned with the worker's validate() contract.
export function videoInputIssue(b: VideoBrief, analysis = b.mode === 'analysis', remake = false, script = ''): string {
  if (analysis) return !b.source.trim() ? '请先添加参考视频' : remake && !b.images.length ? '请先添加用于仿拍的商品图片' : ''
  if (!b.route || !videoRatios[b.route]) return '请选择生成服务'
  const route = b.route
  const resolutions = { grok: ['480p', '720p', '1080p'], minimax: ['768P', '2K'], comfy: ['0.5', '1'] }
  if (!resolutions[route].includes(b.resolution)) return '请选择当前服务支持的生成清晰度'
  if (!videoRatios[route].includes(b.ratio)) return '请选择当前服务支持的画幅'
  const low = { grok: 1, minimax: 4, comfy: 2 }[route]
  if (!Number.isInteger(b.duration) || b.duration < low || b.duration > 15) return `视频时长须为 ${low}–15 秒的整数`
  if (!b.request.trim() && !b.template && !script.trim()) return '请填写拍摄要求或选择模板'
  const count = b.images.length, videos = b.referenceVideos || [], audios = b.referenceAudios || [], voices = b.voiceIds || []
  const mode = !b.inputMode || b.inputMode === 'auto'
    ? count || videos.length || (route === 'grok' && voices.length) ? 'reference' : 'text'
    : b.inputMode
  if (mode === 'text' && (count || videos.length || audios.length || voices.length)) return '文生视频不能使用参考素材，请清空素材或切换模式'
  if (route !== 'minimax' && (mode === 'frames' || videos.length || audios.length || b.firstFrame || b.lastFrame)) return '首尾帧和参考音视频仅支持 MiniMax，请移除不兼容素材'
  if (voices.length && (route !== 'grok' || mode !== 'reference' || b.speechMode === 'silent')) return '音色仅适用于非静音的 Grok 参考生成'
  if (voices.length > 3) return '最多选择 3 个音色'
  if (count > (route === 'grok' ? mode === 'reference' ? 7 : 1 : route === 'minimax' ? 9 : 3)) return '参考图片数量超出当前服务限制'
  if (mode === 'image' && count !== 1) return '单图模式需要恰好一张图片'
  if (route === 'grok' && mode === 'reference') {
    if (!count && !voices.length) return '参考生成需要图片或音色'
    if (b.resolution === '1080p') return 'Grok 参考生成最高支持 720p，请调整清晰度'
  }
  if (mode === 'frames') {
    const frames = [b.firstFrame, b.lastFrame].filter(Boolean)
    if (!frames.length || frames.some(p => !b.images.includes(p!)) || b.images.some(p => !frames.includes(p)) || videos.length || audios.length) return '请只保留并选定首尾帧图片，不能混用其他参考素材'
  } else if (b.firstFrame || b.lastFrame) return '请切换首尾帧模式或清空首尾帧'
  if (videos.length > 3 || audios.length > 3 || count + videos.length + audios.length > 12) return '参考视频、音频各最多 3 个，参考素材总计最多 12 个'
  if (audios.length && !count && !videos.length) return '参考音频需要同时提供图片或视频'
  return ''
}

export function videoSetupIssue(b: VideoBrief, data: VideoBootstrap): string {
  if (!b.route) return ''
  if (b.route !== 'comfy' && data.config[b.route]?.ready === false) return '请先在视频设置中配置当前服务的 API Key'
  if (b.route === 'comfy' && data.dependencies.comfy === false) return 'ComfyUI 运行依赖未就绪，请检查视频设置'
  return ''
}
