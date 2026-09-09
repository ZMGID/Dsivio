export type TaskGroup = 'draft' | 'running' | 'attention' | 'ready'
export type TaskOrganization = { archived: boolean; pinned: boolean }
export type TaskOrganizations = Record<string, TaskOrganization>
export type TaskOrganizationPatch = Partial<TaskOrganization>
export type LibraryTask = {
  id: string
  name: string
  description: string
  kind: string
  kindLabel: string
  status: string
  group: TaskGroup
  updatedAt: number
  detail: string
  error?: string
  canArchive: boolean
}
export type TaskFilter = 'all' | TaskGroup | 'archived'
export type TaskSort = 'newest' | 'oldest' | 'name'
export function taskIsArchived(task: LibraryTask, organization: TaskOrganizations) {
  // A previously archived task resumed from chat must remain visible while active.
  return organization[task.id]?.archived === true && task.canArchive
}
export const taskFilters: { id: TaskFilter; label: string }[] = [
  { id: 'all', label: '全部' }, { id: 'running', label: '进行中' },
  { id: 'attention', label: '需处理' }, { id: 'draft', label: '草稿 / 待生成' },
  { id: 'ready', label: '已有结果' }, { id: 'archived', label: '已归档' },
]
export function selectLibraryTasks(tasks: LibraryTask[], organization: TaskOrganizations, filter: TaskFilter, query: string, kind: string, sort: TaskSort) {
  const words = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)
  return tasks.filter(t => {
    const archived = taskIsArchived(t, organization)
    if (filter === 'archived' ? !archived : archived) return false
    if (filter !== 'all' && filter !== 'archived' && t.group !== filter) return false
    if (kind && t.kind !== kind) return false
    const text = `${t.name} ${t.description} ${t.id} ${t.kindLabel}`.toLocaleLowerCase()
    return words.every(word => text.includes(word))
  }).sort((a, b) => {
    const pin = Number(organization[b.id]?.pinned || false) - Number(organization[a.id]?.pinned || false)
    if (pin) return pin
    if (sort === 'name') return a.name.localeCompare(b.name, 'zh-CN') || a.id.localeCompare(b.id)
    const time = sort === 'oldest' ? a.updatedAt - b.updatedAt : b.updatedAt - a.updatedAt
    return time || a.id.localeCompare(b.id)
  })
}
