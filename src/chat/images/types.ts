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
}
export type ImageResult = {
  id: string
  productId: string
  slotId: string
  revision: number
  path: string | null
  error: string | null
  remoteId: string | null
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
    label: '快速出图',
    description: '主图、白底、场景与局部改图',
    step: '上传参考图，写下要求，让 Agent 把想法整理成可执行的画面。',
  },
  {
    id: 'workflow',
    label: '制作与试品',
    description: '给图制作、换品试做、反馈修正、持续出图',
    step: '把要求变成可复用的规则，用其他商品试做，调整满意后继续生成。',
  },
  {
    id: 'replace',
    label: '样图换货',
    description: '保留版式，替换成你的商品',
    step: '选择已有样图模板，核对商品正反面。先生成两款样品，确认后再铺量。',
  },
  {
    id: 'smart',
    label: '模板套图',
    description: '沿用风格，逐款设计内容',
    step: '选一套风格规则，让 Agent 为每个商品的每一页编写专属方案。',
  },
  {
    id: 'design',
    label: '从零设计',
    description: '没有模板，也能做完整套图',
    step: '给出商品、市场和风格要求。先审阅页面方案，再把成图沉淀为模板。',
  },
  {
    id: 'client',
    label: '多品类批量',
    description: '分类、选模板、分批交付',
    step: '按商品文件夹导入素材，识别分类后检查分组。每个分类单独打样、确认和批量。',
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
export const emptyBrief = (feature: ImageFeature): ImageBrief => ({
  feature,
  name: '',
  requirement: '',
  language: 'pt-BR',
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
