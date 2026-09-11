export const IMAGE_RATIOS = ['1:1', '2:3', '3:4', '4:5', '9:16', '3:2', '4:3', '5:4', '16:9'] as const
export const IMAGE_RESOLUTIONS = ['1k', '2k', '4k'] as const

export type ImageOutputSpec = {
  ratio: string
  resolution: string
  width: number
  height: number
}

const spec = (ratio: string, resolution: string, width = 0, height = 0): ImageOutputSpec => ({
  ratio,
  resolution,
  width,
  height,
})

/** OpenAI image-prompting guide common sizes + the documented 16:9 1K example `1536x864`. */
const GPT_IMAGE_2 = [
  spec('1:1', '1k', 1024, 1024),
  spec('1:1', '2k', 2048, 2048),
  spec('2:3', '1k', 1024, 1536),
  spec('3:2', '1k', 1536, 1024),
  spec('9:16', '1k', 864, 1536),
  spec('9:16', '2k', 1152, 2048),
  spec('9:16', '4k', 2160, 3840),
  spec('16:9', '1k', 1536, 864),
  spec('16:9', '2k', 2048, 1152),
  spec('16:9', '4k', 3840, 2160),
]

/** gpt-image-1 / 1.5 / mini official enum. */
const GPT_IMAGE_1 = [
  spec('1:1', '1k', 1024, 1024),
  spec('2:3', '1k', 1024, 1536),
  spec('3:2', '1k', 1536, 1024),
]

const DALL_E_3 = [
  spec('1:1', '1k', 1024, 1024),
  spec('9:16', '1k', 1024, 1792),
  spec('16:9', '1k', 1792, 1024),
]

/** xAI Imagine: aspect_ratio + resolution `1k` | `2k`. */
const GROK_RATIOS = ['1:1', '2:3', '3:4', '9:16', '3:2', '4:3', '16:9'] as const
const GROK = GROK_RATIOS.flatMap((ratio) => [spec(ratio, '1k'), spec(ratio, '2k')])

const GEMINI_RATIOS = ['1:1', '3:4', '9:16', '4:3', '16:9'] as const
const GEMINI = GEMINI_RATIOS.flatMap((ratio) => [spec(ratio, '1k'), spec(ratio, '2k')])

const GENERIC = ['1:1', '9:16', '16:9'].flatMap((ratio) => [spec(ratio, '1k'), spec(ratio, '2k')])

export function imageOutputKey(ratio: string, resolution: string): string {
  return `${resolution}:${ratio}`
}

export function parseImageOutputKey(value: string): { ratio: string; resolution: string } | null {
  const sep = value.indexOf(':')
  if (sep <= 0) return null
  const resolution = value.slice(0, sep)
  const ratio = value.slice(sep + 1)
  return ratio && resolution ? { ratio, resolution } : null
}

export function listImageOutputs(model?: string, protocol?: string): ImageOutputSpec[] {
  const name = (model || '').toLowerCase()
  if (name.includes('gpt-image-2')) return GPT_IMAGE_2
  if (name.includes('dall-e-3')) return DALL_E_3
  if (name.includes('gpt-image') || name.startsWith('dall-e')) return GPT_IMAGE_1
  if (protocol === 'grok' || name.includes('grok')) return GROK
  if (protocol === 'gemini' || protocol === 'gemini-chat' || name.includes('gemini') || name.startsWith('imagen')) {
    return GEMINI
  }
  if (protocol === 'openai') return GPT_IMAGE_1
  return GENERIC
}

export function imageOutputSpec(ratio: string, resolution: string, model?: string, protocol?: string): ImageOutputSpec {
  const key = imageOutputKey(ratio, resolution)
  return (
    listImageOutputs(model, protocol).find((item) => imageOutputKey(item.ratio, item.resolution) === key)
    || spec(ratio, resolution, ratio === '1:1' ? 1024 : 0, ratio === '1:1' ? 1024 : 0)
  )
}

export function imageOutputPixels(ratio: string, resolution: string, model?: string, protocol?: string): [number, number] {
  const item = imageOutputSpec(ratio, resolution, model, protocol)
  return [item.width, item.height]
}

export function imageOutputShape(ratio: string): string {
  return ratio === '1:1' ? '正方形' : ratio
}

export function imageRatioLabel(ratio: string): string {
  if (ratio === '1:1') return '1:1 正方形'
  return `${ratio} ${imageOutputGroup(ratio)}`
}

export function imageOutputGroup(ratio: string): string {
  const [width, height] = ratio.split(':').map(Number)
  if (!width || !height || width === height) return '正方形'
  return width < height ? '竖版' : '横版'
}

export function imageOutputLabel(item: ImageOutputSpec): string {
  const shape = imageOutputShape(item.ratio)
  if (item.width && item.height) return `${item.width}×${item.height} · ${shape}`
  return `${item.resolution.toUpperCase()} · ${shape}`
}

export function isAllowedImageOutput(
  ratio: string,
  resolution: string,
  model?: string,
  protocol?: string,
): boolean {
  const key = imageOutputKey(ratio, resolution)
  return listImageOutputs(model, protocol).some((item) => imageOutputKey(item.ratio, item.resolution) === key)
}

export function imageOutputChoices(
  model: string | undefined,
  protocol: string | undefined,
  ratio: string,
  resolution: string,
): ImageOutputSpec[] {
  const allowed = listImageOutputs(model, protocol)
  const key = imageOutputKey(ratio, resolution)
  if (allowed.some((item) => imageOutputKey(item.ratio, item.resolution) === key)) return allowed
  return [...allowed, imageOutputSpec(ratio, resolution, model, protocol)]
}

function unique<T>(items: T[]): T[] {
  return [...new Set(items)]
}

export function imageOutputRatios(model?: string, protocol?: string, current?: string): string[] {
  const ratios = unique(listImageOutputs(model, protocol).map((item) => item.ratio))
  return current && !ratios.includes(current) ? [...ratios, current] : ratios
}

export function imageOutputResolutions(
  model: string | undefined,
  protocol: string | undefined,
  ratio: string,
  resolution?: string,
): ImageOutputSpec[] {
  const matched = listImageOutputs(model, protocol).filter((item) => item.ratio === ratio)
  const options = matched.length ? matched : [imageOutputSpec(ratio, resolution || '1k', model, protocol)]
  if (resolution && !options.some((item) => item.resolution === resolution)) {
    return [...options, imageOutputSpec(ratio, resolution, model, protocol)]
  }
  return options
}

export function snapImageOutput(
  model: string | undefined,
  protocol: string | undefined,
  ratio: string,
  resolution: string,
): { ratio: string; resolution: string } {
  if (isAllowedImageOutput(ratio, resolution, model, protocol)) return { ratio, resolution }
  const sameRatio = listImageOutputs(model, protocol).find((item) => item.ratio === ratio)
  if (sameRatio) return { ratio: sameRatio.ratio, resolution: sameRatio.resolution }
  const first = listImageOutputs(model, protocol)[0]
  return { ratio: first.ratio, resolution: first.resolution }
}

export function imageResolutionLabel(item: ImageOutputSpec): string {
  const tier = item.resolution.toUpperCase()
  return item.width && item.height ? `${tier} · ${item.width}×${item.height}` : tier
}
