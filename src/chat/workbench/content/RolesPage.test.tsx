import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import type { Role } from '../../../generated/roles'
import { RolesPage } from './RolesPage'

vi.mock('../../../api/tauri', () => ({
  api: { rolesList: vi.fn(), rolesSave: vi.fn(), rolesDelete: vi.fn() },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))

function role(partial: Partial<Role> = {}): Role {
  return {
    id: 'role-1',
    revision: 1,
    name: '小美',
    description: '出镜',
    images: ['/tmp/a.png'],
    createdAt: '2026-10-02T00:00:00Z',
    updatedAt: '2026-10-02T00:00:00Z',
    ...partial,
  }
}

beforeEach(() => {
  vi.mocked(api.rolesList).mockReset()
  vi.mocked(api.rolesSave).mockReset()
  vi.mocked(api.rolesDelete).mockReset()
  vi.mocked(api.rolesList).mockResolvedValue([])
})

it('saves a role and deletes it', async () => {
  vi.mocked(api.rolesSave).mockResolvedValue(role())
  render(<RolesPage />)
  expect(await screen.findByText('还没有保存的角色')).toBeInTheDocument()
  fireEvent.change(screen.getByRole('textbox', { name: '名字' }), { target: { value: '小美' } })
  fireEvent.click(screen.getByRole('button', { name: '保存角色' }))
  await waitFor(() => expect(api.rolesSave).toHaveBeenCalledWith(expect.objectContaining({
    name: '小美', id: null, revision: null, images: [],
  })))
  expect(await screen.findByRole('button', { name: '小美' })).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: '删除 小美' }))
  await waitFor(() => expect(api.rolesDelete).toHaveBeenCalledWith('role-1'))
  await waitFor(() => expect(screen.queryByRole('button', { name: '小美' })).toBeNull())
})

it('keeps the newer list when an older load resolves late', async () => {
  let resolveFirst: (value: Role[]) => void = () => {}
  vi.mocked(api.rolesList).mockImplementationOnce(() => new Promise((resolve) => { resolveFirst = resolve }))
  render(<RolesPage />)
  await waitFor(() => expect(api.rolesList).toHaveBeenCalledTimes(1))
  vi.mocked(api.rolesList).mockResolvedValueOnce([role({ id: 'new', name: '新角色' })])
  fireEvent.click(screen.getByRole('button', { name: '刷新' }))
  expect(await screen.findByRole('button', { name: '新角色' })).toBeInTheDocument()
  await act(async () => { resolveFirst([role({ name: '旧角色' })]) })
  expect(screen.getByRole('button', { name: '新角色' })).toBeInTheDocument()
  expect(screen.queryByRole('button', { name: '旧角色' })).toBeNull()
})

it('loads again after leaving and reopening', async () => {
  const view = render(<RolesPage />)
  await waitFor(() => expect(api.rolesList).toHaveBeenCalledTimes(1))
  view.unmount()
  vi.mocked(api.rolesList).mockResolvedValue([role()])
  render(<RolesPage />)
  expect(await screen.findByRole('button', { name: '小美' })).toBeInTheDocument()
  expect(api.rolesList).toHaveBeenCalledTimes(2)
})

it('shows a revision conflict and does not pretend the save worked', async () => {
  vi.mocked(api.rolesList).mockResolvedValue([role()])
  vi.mocked(api.rolesSave).mockRejectedValue(new Error('此角色已在其他窗口更新，请重新打开后操作'))
  render(<RolesPage />)
  fireEvent.click(await screen.findByRole('button', { name: '小美' }))
  fireEvent.click(screen.getByRole('button', { name: '保存角色' }))
  expect(await screen.findByText(/重新打开/)).toBeInTheDocument()
})
