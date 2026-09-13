type ExecutionView = { status?: unknown; error?: unknown; requiresReview?: unknown; resolution?: unknown; recovery?: unknown }
const obj = (value: unknown): Record<string, unknown> => value && typeof value === 'object' ? value as Record<string, unknown> : {}
export const subAgentActive = (run?: ExecutionView) => ['running', 'finishing', 'stopping'].includes(String(run?.status))
export const subAgentNeedsAttention = (run?: ExecutionView) => Boolean(run && !subAgentActive(run) && (obj(run.resolution).outcome === 'blocked' || (!run.resolution && (run.requiresReview === true || run.status === 'failed'))))

/** Execution errors and assignment disposition are separate facts. Never infer acceptance. */
export function subAgentStatusLabel(run: ExecutionView | undefined, lang: 'zh' | 'en'): string {
  const t = (zh: string, en: string) => lang === 'zh' ? zh : en
  const status = String(run?.status ?? 'accepted')
  const active: Record<string, string> = { running: t('运行中', 'Running'), finishing: t('正在收尾', 'Finishing'), stopping: t('正在停止', 'Stopping') }
  if (active[status]) return active[status]
  const outcome = String(obj(run?.resolution).outcome ?? '')
  const resolved: Record<string, string> = { accepted: t('结果已采用', 'Result accepted'), completed_by_parent: t('主代理已补齐', 'Completed by parent'), blocked: t('待继续处理', 'Needs attention'), continued: t('已继续', 'Continued'), reassigned: t('已改派', 'Reassigned'), cancelled: t('已停止', 'Stopped') }
  if (resolved[outcome]) return resolved[outcome]
  if (status === 'failed' && typeof run?.error === 'string' && run.error.startsWith('recovered: ')) return t('有恢复结果', 'Recovered output available')
  if (run?.requiresReview) return status === 'completed' ? t('等待主代理验收', 'Awaiting parent review') : t('等待主代理处理', 'Awaiting parent decision')
  return ({ completed: t('已完成', 'Completed'), failed: t('执行异常 · 待处理', 'Execution error · Needs attention'), interrupted: t('已中断', 'Interrupted'), accepted: t('派工已受理', 'Assignment accepted') } as Record<string, string>)[status] ?? status
}
