import { convertFileSrc } from '@tauri-apps/api/core'
import { api } from '../../api/tauri'
import type { MediaModelInfo, MediaRequest as GatewayRequest, MediaTask } from '../../generated/mediaGeneration'

export type MediaKind = 'image' | 'video'
/** Kivio's composer fields projected onto the existing Dsivio request, without a second task store. */
export type MediaRequest = {
  kind: MediaKind; providerId: string; model: string; prompt: string
  aspectRatio: string; duration: number; referencePaths: string[]
  options?: Record<string, unknown>
}
export type MediaJob = {
  id: string; createdAt: number; request: MediaRequest
  status: 'running' | 'completed' | 'failed' | 'cancelled'
  error: string | null; providerTaskId?: string; reuseError?: string
  outputs: { name: string; mimeType: string; preview: string }[]
}
// Immutable request snapshots need one IPC read per task, rather than one per status poll.
const requests = new Map<string, Promise<GatewayRequest>>()
function savedRequest(id: string) {
  let request = requests.get(id)
  if (!request) {
    request = api.mediaTaskRequest(id).catch(error => { requests.delete(id); throw error })
    requests.set(id, request)
  }
  return request
}
export async function toMediaJob(task: MediaTask): Promise<MediaJob> {
  let saved: GatewayRequest | undefined, reuseError: string | undefined
  try { saved = await savedRequest(task.id) } catch (error) { reuseError = String(error) }
  const options = saved?.options ?? {}
  const references = task.kind === 'image' ? saved?.images : options.firstFrame
  return {
    id: task.id, createdAt: Date.parse(task.createdAt), status: task.status === 'succeeded' ? 'completed' : task.status,
    error: task.error, providerTaskId: task.canResume ? task.remoteId ?? task.id : undefined, reuseError,
    request: { kind: task.kind as MediaKind, providerId: task.providerId, model: task.model, prompt: saved?.prompt ?? task.prompt,
      aspectRatio: String(options.aspectRatio ?? options.ratio ?? ''), duration: Number(options.duration ?? 0),
      referencePaths: Array.isArray(references) ? references as string[] : typeof references === 'string' ? [references] : [], options },
    outputs: task.outputs.map(output => ({ name: output.path.split(/[\\/]/).pop() ?? 'output', mimeType: output.mime, preview: convertFileSrc(output.path) })),
  }
}
export async function buildStationRequest(request: MediaRequest, described?: MediaModelInfo): Promise<GatewayRequest> {
  const info = described ?? await api.describeMediaModel(request.providerId, request.model, request.kind)
  const supports = (key: string) => info.parameters.some(p => p.key === key)
  if (!supports('prompt')) throw new Error('此工作流未绑定通用提示词，请先在模型设置中配置提示词输入绑定，或使用工作台的工作流表单。')
  const options = { ...request.options }
  const ratioKey = request.kind === 'image' ? 'aspectRatio' : 'ratio'
  // The visible composer fields replace reused values, including an explicit model default.
  delete options[ratioKey]
  if (request.kind === 'image') delete options.aspect_ratio
  if (request.kind === 'video') delete options.duration
  if (supports(ratioKey) && request.aspectRatio) options[ratioKey] = request.aspectRatio
  if (request.kind === 'video' && supports('duration') && request.duration) options.duration = request.duration
  const referenceKey = request.kind === 'image' ? 'images' : 'firstFrame'
  if (request.referencePaths.length && !supports(referenceKey)) throw new Error('当前模型不支持这些参考图片，请移除参考图或切换模型。')
  if (request.kind === 'video') {
    delete options.firstFrame
    if (request.referencePaths.length) options.firstFrame = request.referencePaths
  }
  return { kind: request.kind, providerId: request.providerId, model: request.model,
    prompt: request.prompt, images: request.kind === 'image' ? request.referencePaths : [],
    options, descriptionRevision: info.revision, origin: 'media-station' }
}
export const mediaStationApi = {
  async delete(ids: string[]) {
    const deletedIds: string[] = [], failures: { id: string; error: string }[] = []
    for (const id of ids) {
      try { await api.deleteMediaTask(id); requests.delete(id); deletedIds.push(id) }
      catch (error) { failures.push({ id, error: String(error) }) }
    }
    return { deletedIds, failures }
  },
  async reference(id: string, index: number) {
    const task = await api.getMediaTask(id)
    const output = task.outputs[index]
    if (!output?.mime.startsWith('image/')) throw new Error('图片源文件不可用')
    return output.path
  },
  export: (id: string, index: number, destination: string) => api.exportMediaOutput(id, destination, index),
}
