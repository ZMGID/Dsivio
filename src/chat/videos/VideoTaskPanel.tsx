import { useMemo } from 'react'
import type { VideoTask } from './types'
import { TaskLibrary } from '../studio/TaskLibrary'
import { videoLibraryTask } from '../studio/taskAdapters'
import type { TaskLibraryState } from '../studio/useTaskLibrary'

export function VideoTaskPanel({ tasks, library, loading, disabled, currentId, onOpen, onRefresh, onNew }: {
  tasks: VideoTask[]; library: TaskLibraryState; loading: boolean; disabled?: boolean; currentId?: string
  onOpen: (task: VideoTask) => void; onRefresh: () => Promise<void>; onNew: () => void
}) {
  const entries = useMemo(() => tasks.map(videoLibraryTask), [tasks])
  return <TaskLibrary noun="视频" tasks={entries} library={library} loading={loading} disabled={disabled} currentId={currentId}
    onOpen={id => { const task = tasks.find(t => t.id === id); if (task) onOpen(task) }} onRefresh={onRefresh} onNew={onNew} />
}
