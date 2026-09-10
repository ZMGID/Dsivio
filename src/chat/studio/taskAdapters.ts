import { FEATURES, latestResults, type ImageTask } from '../images/types'
import { videoStatus, type VideoTask } from '../videos/types'
import type { LibraryTask, TaskGroup } from './taskLibraryModel'

export function imageLibraryTask(task: ImageTask): LibraryTask {
  const labels: Record<string, string> = { draft: '草稿', planned: '方案待确认', running: '生成中', ready: '本步骤已完成', stopped: '已停止', interrupted: '已中断', error: '生成失败' }
  const group: TaskGroup = task.status === 'running' ? 'running'
    : ['error', 'stopped', 'interrupted'].includes(task.status) ? 'attention'
    : task.status === 'ready' && task.results.some(r => r.path) ? 'ready' : 'draft'
  const results = latestResults(task).filter(r => r.path).length
  return {
    id: task.id, name: task.brief.name || '未命名图片任务', description: task.brief.requirement,
    kind: task.brief.feature, kindLabel: FEATURES.find(f => f.id === task.brief.feature)?.label || '图片创作',
    status: labels[task.status] || task.status, group, updatedAt: Date.parse(task.updatedAt) || 0,
    detail: [results ? `${results} 张结果` : '', task.progress].filter(Boolean).join(' · '),
    error: task.error || undefined, canDelete: task.status !== 'running',
  }
}
export function videoLibraryTask(task: VideoTask): LibraryTask {
  const analysis = task.brief.mode === 'analysis'
  const needsAttention = ['failed', 'uncertain'].includes(task.status) || !!task.submission?.retryable
  const group: TaskGroup = needsAttention ? 'attention'
    : ['running', 'submitting'].includes(task.status) ? 'running'
    : task.status === 'succeeded' || (analysis && !!task.script) ? 'ready' : 'draft'
  const route = ({ grok: 'Grok', minimax: 'MiniMax', comfy: 'ComfyUI', '': '未选路线' })[task.brief.route]
  return {
    id: task.id, name: task.brief.name || '未命名视频任务', description: task.brief.request || task.brief.source,
    kind: analysis ? 'analysis' : task.brief.route || 'creation', kindLabel: analysis ? '视频拆解 / 参考' : `${route} 视频`,
    status: task.submission?.retryable ? '生成被拒绝 · 可重试' : analysis && task.script && group === 'ready' ? '已拆解' : videoStatus[task.status] || task.status,
    group, updatedAt: Number(task.updatedAt) || 0,
    detail: analysis ? '参考分析' : [task.brief.duration ? `${task.brief.duration} 秒` : '', task.brief.ratio, task.brief.resolution, task.output ? '成片已保存' : ''].filter(Boolean).join(' · '),
    error: task.error || task.submission?.reason,
    canDelete: !['running', 'submitting', 'uncertain'].includes(task.status),
  }
}
