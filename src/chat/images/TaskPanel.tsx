import { FEATURES, type ImageTask } from './types'

function featureLabel(id: string) {
  return FEATURES.find((feature) => feature.id === id)?.label || '图片任务'
}

function statusLabel(status: string) {
  if (status === 'running') return '进行中'
  if (status === 'error') return '失败'
  if (status === 'draft') return '草稿'
  return '已保存'
}

function when(value: string) {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  return date.toLocaleString('zh-CN', { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

export function TaskPanel({
  tasks,
  loading,
  currentId,
  onOpen,
}: {
  tasks: ImageTask[]
  loading: boolean
  currentId?: string
  onOpen: (task: ImageTask) => void
}) {
  return (
    <div className="is-template-page">
      <div className="is-section-heading">
        <div>
          <h2>任务</h2>
          <span className="is-count">{tasks.length}</span>
        </div>
      </div>
      <p className="is-muted">保存过的制作会留在这里，点开就能接着做。</p>
      {loading && <p className="is-muted">正在加载…</p>}
      {!loading && tasks.length === 0 && (
        <p className="iw-empty">还没有保存的任务。在左侧选择一种做法，开始创作后会自动保存在这里。</p>
      )}
      <div className="is-task-list">
        {tasks.map((task) => {
          const stamp = when(task.updatedAt)
          return (
            <button
              type="button"
              key={task.id}
              className={`is-task-card${currentId === task.id ? ' active' : ''}`}
              aria-label={task.brief.name || '未命名任务'}
              onClick={() => onOpen(task)}
            >
              <span className={`is-history-dot ${task.status}`} />
              <span>
                <strong>{task.brief.name || '未命名任务'}</strong>
                <small>
                  {featureLabel(task.brief.feature)}
                  {task.progress ? ` · ${task.progress}` : ''}
                  {stamp ? ` · ${stamp}` : ''}
                </small>
              </span>
              <b>{statusLabel(task.status)}</b>
            </button>
          )
        })}
      </div>
    </div>
  )
}
