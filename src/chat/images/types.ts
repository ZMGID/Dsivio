export type ImageFeature = 'gen' | 'replace' | 'smart' | 'design' | 'client' | 'workflow'
export type ImageAsset = { id: string; name: string; path: string }
export type ImageProduct = {
  id: string
  name: string
  category: string
  kind: string
  facts: string
  front: string | null
  back: string | null
  assets: ImageAsset[]
  templateId: string | null
}
export type ImageBrief = {
  feature: ImageFeature
  name: string
  requirement: string
  language: string
  platform: string
  ratio: string
  resolution: string
  count: number
  style: string
  templateId: string | null
  products: ImageProduct[]
  workflowInput?: { mode: 'smart' | 'replace'; sources: ImageAsset[] } | null
}
export type ImagePlan = {
  productId: string
  slotId: string
  purpose: string
  copy: string
  prompt: string
  refs: string[]
}
export type ImageConfig = {
  providerId: string
  model: string
  protocol: string
  agentProviderId: string
  agentModel: string
  outputRoot?: string
}
export type ImageResult = {
  id: string
  productId: string
  slotId: string
  revision: number
  path: string | null
  error: string | null
  remoteId: string | null
  downloadUrl?: string | null
  prompt: string
  width: number
  height: number
  review: string | null
  config: ImageConfig
}
export type ImageTemplateSlot = {
  id: string
  purpose?: string
  brief?: string
  prompt?: string
  example?: string
  refs?: string[]
  [key: string]: unknown
}
export type ImageTemplate = {
  id: string
  directory: string
  builtin: boolean
  data: {
    name: string
    mode: 'smart' | 'replace'
    category?: string
    language?: string
    style?: string
    output?: { ratio?: string; resolution?: string; [key: string]: unknown }
    slots: ImageTemplateSlot[]
    product_kinds?: Record<string, unknown>
    [key: string]: unknown
  }
}
export type ImageTask = {
  id: string
  revision: number
  createdAt: string
  updatedAt: string
  brief: ImageBrief
  plans: ImagePlan[]
  results: ImageResult[]
  approvedGroups: string[]
  status: string
  progress: string
  error: string | null
  templates: ImageTemplate[]
  outputDirectory?: string | null
  workflow?: ImageWorkflow | null
}
export type ImageWorkflow = {
  ruleVersion: number
  rulesCurrent: boolean
  approvedVersion: number | null
  sampleIds: string[]
  changes: {
    version: number
    revision: number
    note: string
    summary: string
    createdAt: string
    template: ImageTemplate
  }[]
}
export type ImageProvider = { id: string; name: string; models: string[]; ready: boolean }
export type ImageBootstrap = {
  tasks: ImageTask[]
  templates: ImageTemplate[]
  config: ImageConfig
  providers: ImageProvider[]
}
export type ImageAction = {
  kind: string
  group?: string
  productId?: string
  slotId?: string
  resultId?: string
  note?: string
  sampleIds?: string[]
  templateData?: ImageTemplate['data']
}

export const FEATURES = [
  {
    id: 'gen',
    label: '单张 / 改图',
    description: '主图、白底、场景与局部改图',
    step: '生成一张图片，或修改已有图片。写下要求即可开始。',
  },
  {
    id: 'workflow',
    label: '制作模板',
    description: '给图制作、换品试做、反馈修正、持续出图',
    step: '把要求变成可复用的规则，用其他商品试做，调整满意后继续生成。',
  },
  {
    id: 'replace',
    label: '样图换货',
    description: '保留版式，替换成你的商品',
    step: '沿用现成套图的版式，只把商品换成你的。',
  },
  {
    id: 'smart',
    label: '模板套图',
    description: '沿用风格，逐款设计内容',
    step: '选择已有模板，为你的商品生成同一风格的整套图片。',
  },
  {
    id: 'design',
    label: '从零设计',
    description: '没有模板，也能做完整套图',
    step: '没有模板也没关系，放入商品，描述你想要的一套图。',
  },
  {
    id: 'client',
    label: '多品类批量',
    description: '分类、选模板、分批交付',
    step: '导入一批商品，自动分类、安排套图，每类先出样品。',
  },
] as const

export const SHOTS = [
  '白底/纯色底产品主图',
  '场景化生活图',
  '平铺图',
  '细节微距图',
  '促销海报/Banner',
  '社交媒体素材',
  'UGC风格/买家秀',
  '模特展示图',
  '使用前后对比图',
  '包装设计展示',
  '信息图/A+Content',
  '创意概念广告图',
  '尺寸规格+使用步骤图',
  '多产品套装/组合展示',
  '电商直播间场景',
  '虚拟试穿/产品融入场景',
  '技术拆解/爆炸图',
  '隐形模特',
  '产品多角度网格',
  '杂志大片/封面',
  '季节主题网格',
  '奢华氛围渲染',
  '设备界面模型',
  '店铺门面/空间摄影',
  '运动/健身广告',
  '箱包功能证据图',
]
const LANGUAGE_MARKET: Record<string, string> = {
  'pt-BR': '巴西市场',
  'en-US': '美国市场',
  'en-GB': '英国市场',
  es: '西语市场',
  'zh-CN': '中国市场',
  'zh-TW': '台湾市场',
  ja: '日本市场',
  ko: '韩国市场',
  fr: '法国市场',
  de: '德国市场',
  it: '意大利市场',
  ar: '阿语市场',
  id: '印尼市场',
  th: '泰国市场',
  vi: '越南市场',
}

/** Empty names become `{platform or product} · {market}` on save, matching the field example. */
export function suggestImageTaskName(brief: ImageBrief): string {
  const named = brief.products
    .map((product) => product.name.trim())
    .find((name) => name && name !== '商品素材')
  const head =
    named ||
    brief.platform.trim() ||
    FEATURES.find((feature) => feature.id === brief.feature)?.label ||
    '图片任务'
  const language = brief.language.trim()
  const market =
    LANGUAGE_MARKET[language] || (language && language !== '无文字' ? language : '')
  return market ? `${head} · ${market}` : head
}

export const emptyBrief = (feature: ImageFeature): ImageBrief => ({
  feature,
  name: '',
  requirement: '',
  language: feature === 'gen' ? '无文字' : feature === 'replace' ? '跟随样图' : 'zh-CN',
  platform: '通用电商',
  ratio: '1:1',
  resolution: '1k',
  count: feature === 'gen' ? 1 : 7,
  style: '',
  templateId: null,
  products: [],
  ...(feature === 'workflow' ? { workflowInput: { mode: 'smart' as const, sources: [] } } : {}),
})
export const productGroup = (p: ImageProduct) => p.category.trim() || '未分类'
export function latestResults(task: ImageTask): ImageResult[] {
  const map = new Map<string, ImageResult>()
  for (const r of task.results)
    if (r.revision === task.revision) map.set(`${r.productId}/${r.slotId}`, r)
  return [...map.values()]
}
export function sampleComplete(task: ImageTask, group: string): boolean {
  const ids = task.brief.products
    .filter((p) => productGroup(p) === group)
    .slice(0, 2)
    .map((p) => p.id)
  const results = latestResults(task)
  return (
    ids.length > 0 &&
    ids.every((id) => {
      const plans = task.plans.filter((p) => p.productId === id)
      return (
        plans.length > 0 &&
        plans.every((p) =>
          results.some((r) => r.productId === id && r.slotId === p.slotId && !!r.path),
        )
      )
    })
  )
}

export function workflowSamplesComplete(task: ImageTask): boolean {
  const workflow = task.workflow
  const slots = task.templates[0]?.data.slots || []
  const results = latestResults(task)
  return (
    !!workflow?.rulesCurrent &&
    workflow.sampleIds.length > 0 &&
    slots.length > 0 &&
    workflow.sampleIds.every(
      (id) =>
        task.brief.products.some((p) => p.id === id) &&
        slots.every((slot) =>
          results.some((r) => r.productId === id && r.slotId === slot.id && !!r.path),
        ),
    )
  )
}

export function workflowInputsChanged(brief: ImageBrief, saved: ImageBrief): boolean {
  return (
    [
      'workflowInput',
      'requirement',
      'language',
      'platform',
      'ratio',
      'resolution',
      'count',
      'style',
    ] as const
  ).some((key) => JSON.stringify(brief[key]) !== JSON.stringify(saved[key]))
}
