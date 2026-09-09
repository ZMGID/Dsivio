import { useMemo } from 'react'
import type { ImageTask } from './types'
import { TaskLibrary } from '../studio/TaskLibrary'
import { imageLibraryTask } from '../studio/taskAdapters'
import type { TaskLibraryState } from '../studio/useTaskLibrary'

export function TaskPanel({ tasks, loading, currentId, onOpen, onRefresh, onNew, library, disabled }: {
  tasks: ImageTask[]; loading: boolean; currentId?: string; onOpen: (task: ImageTask) => void
  onRefresh: () => Promise<void>; onNew: () => void; library: TaskLibraryState; disabled?: boolean
}) {
  const entries = useMemo(() => tasks.map(imageLibraryTask), [tasks])
  return <TaskLibrary noun="图片" tasks={entries} library={library} loading={loading} disabled={disabled} currentId={currentId}
    onOpen={id => { const task = tasks.find(t => t.id === id); if (task) onOpen(task) }} onRefresh={onRefresh} onNew={onNew} />
}
