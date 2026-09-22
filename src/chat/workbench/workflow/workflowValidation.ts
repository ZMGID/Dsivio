import type { Settings } from '../../../api/tauri'
import { api, isTauriRuntime } from '../../../api/tauri'
import { getSettingsCached } from '../../../api/settingsCache'
import { isMediaPoolCandidate, mediaPoolEntries } from '../../../data/mediaModelPools'
import { resolveModelInfo } from '../../../data/modelMatching'
import { videoModel } from '../../../data/videoModels'
import { imageOutputKey, listImageOutputs } from '../image/projects/imageOutput'
import { nodeConfig } from './workflowConfig'
import { graphProblems, inputSource } from './workflowGraph'
import type { GenerationWorkflow } from './workflowModel'

export function modelProblems(flow: GenerationWorkflow, settings: Settings): string[] {
  return flow.nodes.flatMap(node => {
    const config = nodeConfig(node)
    if (node.kind === 'text.join' || !('model' in config) || !config.model) return []
    const { providerId, model } = config.model
    const provider = settings.providers.find(p => p.id === providerId)
    const issue = (message: string) => [`${node.title}：${message}`]
    if (!provider || provider.enabled === false) return issue('模型提供商不可用，请重选')
    if (config.type === 'text') return provider.enabledModels.includes(model) ? [] : issue('文本模型不可用，请重选')
    if (config.type === 'understand') return provider.enabledModels.includes(model) && resolveModelInfo(model, provider.modelOverrides, provider).capabilities?.vision ? [] : issue('理解模型不可用或不支持图片')
    const kind = node.kind === 'image.generate' ? 'imageModels' : 'videoModels'
    if (!isMediaPoolCandidate(provider, model, kind) || !mediaPoolEntries(settings, kind).some(e => e.available && e.providerId === providerId && e.model === model)) return issue('生成模型已不在可用模型池中，请重选')
    const comfy = provider.request.comfy?.workflows.find(w => w.id === model)
    if (comfy) {
      const invalid = comfy.inputs.some(binding => {
        const value = config.options[`${binding.nodeId}:${binding.input}`]
        return binding.kind === 'number' && value !== undefined && (!value.trim() || !Number.isFinite(Number(value)))
      })
      return invalid ? issue('模型数字参数无效') : []
    }
    if (node.kind === 'image.generate') return config.options.output && !listImageOutputs(model).some(item => imageOutputKey(item.ratio, item.resolution) === config.options.output) ? issue('图片尺寸参数不在模型支持范围内') : []
    const profile = videoModel(model)
    if (!profile) return []
    for (const [key, values] of [['duration', profile.durations], ['resolution', profile.resolutions], ['ratio', profile.ratios]] as const) {
      if (config.options[key] && !(values as readonly (string | number)[]).map(String).includes(config.options[key])) return issue('视频参数不在模型支持范围内')
    }
    const hasImage = !!inputSource(flow, node.id, 'image') || config.assets.some(a => a.path)
    if (!hasImage && !profile.modes.includes('text')) return issue('该模型需要图片素材')
    if (hasImage && !profile.modes.includes(config.options.imageMode === 'reference' ? 'reference' : 'image')) return issue('该模型不支持所选图片用途，请调整首帧/参考图选项')
    return []
  })
}

/** Read-only validation; never submits AI/media tasks. */
export async function checkWorkflow(flow: GenerationWorkflow): Promise<string[]> {
  const problems = graphProblems(flow)
  if (flow.nodes.some(node => node.kind !== 'text.join' && 'model' in nodeConfig(node))) {
    try { problems.push(...modelProblems(flow, await getSettingsCached())) }
    catch (error) { problems.push(`无法验证模型配置：${String(error)}`) }
  }
  const assets = flow.nodes.flatMap(node => {
    const config = nodeConfig(node)
    return 'assets' in config && !(config.type !== 'assets' && inputSource(flow, node.id, 'image')) ? config.assets.map(asset => ({ ...asset, node: node.title, type: node.kind === 'video.upload' ? 'video' : 'image' })) : []
  })
  if (assets.length && !isTauriRuntime()) problems.push('本机素材需在桌面应用中检查，当前无法确认可访问性')
  else if (assets.length) {
    try {
      const available = await api.chatInspectAttachmentPaths(assets.map(a => a.path).filter(Boolean))
      for (const asset of assets) if (!available.some(item => item.path === asset.path && item.type === asset.type)) problems.push(`${asset.node}：${asset.name} 不可访问或类型错误，请重新选择`)
    } catch (error) { problems.push(`素材检查失败，请重新选择：${String(error)}`) }
  }
  return problems
}
