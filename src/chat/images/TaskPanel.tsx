import { api } from '../../api/tauri'
import { useMemo } from 'react'
import type { ImageTask } from './types'
import { TaskLibrary } from '../studio/TaskLibrary'
import { imageLibraryTask } from '../studio/taskAdapters'
import type { TaskLibraryState } from '../studio/useTaskLibrary'

export function TaskPanel({ activeOperations, tasks, loading, currentId, onOpen, onRefresh, onNew, library, disabled, onDeleted }: {
  onDeleted: (id: string) => void
  activeOperations?: Record<string, boolean>
  tasks: ImageTask[]; loading: boolean; currentId?: string; onOpen: (task: ImageTask) => void
  onRefresh: () => Promise<void>; onNew: () => void; library: TaskLibraryState; disabled?: boolean
}) {
  const entries = useMemo(() => tasks.map(task => {
    const item = imageLibraryTask(task)
    return activeOperations?.[task.id]
      ? { ...item, group: 'running' as const, status: '正在处理图片…', canDelete: false } : item
  }), [tasks, activeOperations])
  return <TaskLibrary noun="图片" tasks={entries} library={library} loading={loading} disabled={disabled} currentId={currentId}
    onReveal={id => api.studioTaskFileAction('image', id, 'reveal')}
    onDelete={async id => { await api.studioTaskFileAction('image', id, 'delete'); onDeleted(id); await onRefresh() }}
    onOpen={id => { const task = tasks.find(t => t.id === id); if (task) onOpen(task) }} onRefresh={onRefresh} onNew={onNew} />
}
