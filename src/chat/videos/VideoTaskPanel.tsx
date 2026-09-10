import { api } from '../../api/tauri'
import { useMemo } from 'react'
import type { VideoTask } from './types'
import { TaskLibrary } from '../studio/TaskLibrary'
import { videoLibraryTask } from '../studio/taskAdapters'
import type { TaskLibraryState } from '../studio/useTaskLibrary'

export function VideoTaskPanel({ activeOperations, tasks, library, loading, disabled, currentId, onOpen, onRefresh, onNew, onDeleted }: {
  onDeleted: (id: string) => void
  activeOperations?: Record<string, string>
  tasks: VideoTask[]; library: TaskLibraryState; loading: boolean; disabled?: boolean; currentId?: string
  onOpen: (task: VideoTask) => void; onRefresh: () => Promise<void>; onNew: () => void
}) {
  const entries = useMemo(() => tasks.map(task => {
    const item = videoLibraryTask(task)
    return activeOperations?.[task.id]
      ? { ...item, group: 'running' as const, status: activeOperations[task.id], canDelete: false } : item
  }), [tasks, activeOperations])
  return <TaskLibrary noun="视频" tasks={entries} library={library} loading={loading} disabled={disabled} currentId={currentId}
    onReveal={id => api.studioTaskFileAction('video', id, 'reveal')}
    onDelete={async id => { await api.studioTaskFileAction('video', id, 'delete'); onDeleted(id); await onRefresh() }}
    onOpen={id => { const task = tasks.find(t => t.id === id); if (task) onOpen(task) }} onRefresh={onRefresh} onNew={onNew} />
}
