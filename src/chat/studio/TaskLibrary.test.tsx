import { useState } from 'react'
import { fireEvent, render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { TaskLibrary } from './TaskLibrary'
import { selectLibraryTasks, type LibraryTask } from './taskLibraryModel'
import { videoLibraryTask, imageLibraryTask } from './taskAdapters'
import { newVideoBrief } from '../videos/types'
import { emptyBrief } from '../images/types'

const tasks: LibraryTask[] = Array.from({ length: 1000 }, (_, i) => ({
  id: `task-${i}`, name: `作品 ${i}`, description: i === 500 ? '夏季新品蓝色背包' : '商品展示',
  kind: i % 2 ? 'grok' : 'comfy', kindLabel: i % 2 ? 'Grok 视频' : 'ComfyUI 视频',
  status: i % 3 ? '草稿' : '生成中', group: i % 3 ? 'draft' : 'running',
  updatedAt: 1700000000000 + i * 1000, detail: '5 秒 · 16:9', canDelete: i % 3 !== 0,
}))

describe('task library at scale', () => {
  it('reveals the selected task and requires confirmation before deleting', async () => {
    const onReveal = vi.fn(async () => {})
    const onDelete = vi.fn(async () => {})
    const onOpen = vi.fn()
    render(<TaskLibrary noun="视频" tasks={tasks.slice(1, 2)} library={{ organization: {}, ready: true, error: '', pending: false, refresh: vi.fn(), update: vi.fn() }} loading={false} onNew={vi.fn()} onOpen={onOpen} onRefresh={vi.fn()} onReveal={onReveal} onDelete={onDelete} />)
    fireEvent.click(screen.getByRole('button', { name: '打开所在文件夹 作品 1' }))
    expect(onReveal).toHaveBeenCalledWith('task-1')
    expect(onOpen).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(screen.getByRole('button', { name: '删除 作品 1' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: '删除 作品 1' }))
    expect(onDelete).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: '取消' }))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '删除 作品 1' }))
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    expect(onDelete).toHaveBeenCalledWith('task-1')
    expect(await screen.findByText('已删除 1 个任务及其专属素材和生成文件。')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: '删除 作品 1' }))
    expect(screen.queryByText('已删除 1 个任务及其专属素材和生成文件。')).toBeNull()
    const dialog = screen.getByRole('alertdialog')
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(within(screen.getByLabelText('视频任务管理')).queryByRole('alertdialog')).toBeNull()
    expect(screen.getByRole('button', { name: '取消' })).toHaveFocus()
    fireEvent.keyDown(dialog, { key: 'Escape' })
    expect(screen.queryByRole('alertdialog')).toBeNull()

  })

  it('searches the complete library while rendering one page, and resets selection across filters', async () => {
    const onOpen = vi.fn()
    render(<TaskLibrary onReveal={vi.fn()} onDelete={vi.fn()} noun="视频" tasks={tasks} library={{ organization: {}, ready: true, error: '', pending: false, refresh: vi.fn(), update: vi.fn() }} loading={false} onNew={vi.fn()} onOpen={onOpen} onRefresh={vi.fn()} />)
    expect(screen.getAllByRole('button', { name: /^打开任务/ })).toHaveLength(24)
    fireEvent.click(screen.getByRole('button', { name: '下一页' }))
    expect(screen.getByText('2 / 42')).toBeTruthy()
    fireEvent.click(screen.getByLabelText('选择本页任务'))
    expect(screen.getByText('已选 24 项')).toBeTruthy()
    fireEvent.change(screen.getByLabelText('搜索任务'), { target: { value: '夏季 背包' } })
    expect(screen.getAllByRole('button', { name: /^打开任务/ })).toHaveLength(1)
    expect(screen.queryByText('已选 24 项')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '打开任务 作品 500' }))
    expect(onOpen).toHaveBeenCalledWith('task-500')
    fireEvent.change(screen.getByLabelText('任务类型'), { target: { value: 'grok' } })
    expect(screen.getByText('没有符合条件的任务')).toBeTruthy()
  })

  it('deletes a selected batch and exposes no archive controls', async () => {
    const onDelete = vi.fn<(id: string) => Promise<void>>(async () => {})
    function Harness() {
      const [items, setItems] = useState(tasks.slice(1, 3))
      return <TaskLibrary onReveal={vi.fn()} onDelete={async id => { await onDelete(id); setItems(items => items.filter(t => t.id !== id)) }} noun="图片" tasks={items} loading={false} onNew={vi.fn()} onOpen={vi.fn()} onRefresh={vi.fn()} library={{ organization: { 'task-1': { archived: true, pinned: false } }, ready: true, pending: false, error: '', refresh: vi.fn(), update: vi.fn() }} />
    }
    render(<Harness />)
    expect(screen.queryByRole('button', { name: /归档|恢复/ })).toBeNull()
    expect(screen.getAllByRole('button', { name: /^打开任务/ })).toHaveLength(2)
    fireEvent.click(screen.getByLabelText('选择本页任务'))
    fireEvent.click(screen.getByRole('button', { name: '删除任务' }))
    expect(onDelete).not.toHaveBeenCalled()
    expect(screen.getByRole('alertdialog')).toHaveTextContent('专属素材副本和生成文件')
    fireEvent.click(screen.getByRole('button', { name: '确认删除' }))
    expect(await screen.findByText('已删除 2 个任务及其专属素材和生成文件。')).toBeTruthy()
    expect(onDelete).toHaveBeenCalledWith('task-1')
    expect(onDelete).toHaveBeenCalledWith('task-2')
    expect(screen.queryByRole('button', { name: /^打开任务/ })).toBeNull()
  })

  it('keeps pending jobs visible and prevents deleting them in a mixed selection', () => {
    render(<TaskLibrary onReveal={vi.fn()} onDelete={vi.fn()} noun="视频" tasks={tasks.slice(0, 2)} library={{ organization: {}, ready: true, error: '', pending: false, refresh: vi.fn(), update: vi.fn() }} loading={false} onNew={vi.fn()} onOpen={vi.fn()} onRefresh={vi.fn()} />)
    fireEvent.click(screen.getByLabelText('选择本页任务'))
    expect(screen.getByRole('button', { name: '删除任务' })).toBeDisabled()
    expect(screen.getByRole('button', { name: '删除 作品 0' })).toBeDisabled()
    expect(within(screen.getByLabelText('视频任务管理')).getByText('进行中或待核查任务需先处理')).toBeTruthy()
  })

  it('prioritizes pinned tasks and restores legacy archived tasks to the list', () => {
    const metadata = { 'task-2': { pinned: true, archived: false }, 'task-4': { pinned: false, archived: true }, 'task-0': { pinned: false, archived: true } }
    const visible = selectLibraryTasks(tasks, metadata, 'all', '', '', 'newest')
    expect(visible[0].id).toBe('task-2')
    expect(visible.some(t => t.id === 'task-4')).toBe(true)
    expect(visible.some(t => t.id === 'task-0')).toBe(true)
  })

  it('classifies rejected video submissions and interrupted image jobs as needing attention', () => {
    const base = { id: 'v', revision: 1, updatedAt: 0, brief: newVideoBrief(), script: '', prompt: '', approved: true, status: 'approved' }
    expect(videoLibraryTask({ ...base, submission: { state: 'rejected', retryable: true, reason: '余额不足' } }).group).toBe('attention')
    expect(videoLibraryTask({ ...base, status: 'uncertain' }).canDelete).toBe(false)
    expect(videoLibraryTask({ ...base, brief: newVideoBrief('analysis'), script: '已分析' }).status).toBe('已拆解')
    expect(imageLibraryTask({ id: 'i', revision: 1, createdAt: '', updatedAt: '', brief: emptyBrief('gen'), plans: [], results: [], approvedGroups: [], status: 'interrupted', progress: '', error: null, templates: [] }).group).toBe('attention')
  })
})
