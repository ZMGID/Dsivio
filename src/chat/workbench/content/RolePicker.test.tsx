import { fireEvent, render, screen } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import type { Role } from '../../../generated/roles'
import { RolePicker, rolesToReferenceImages } from './RolePicker'

vi.mock('../../../api/tauri', () => ({
  api: { rolesList: vi.fn(), rolesSave: vi.fn(), rolesDelete: vi.fn() },
}))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))

const xiaomei: Role = {
  id: '1', revision: 1, name: '小美', description: '', images: ['/abs/a.png', '/abs/b.png'],
  createdAt: '2026-10-02T00:00:00Z', updatedAt: '2026-10-02T00:00:00Z',
}
const xiaoming: Role = {
  id: '2', revision: 1, name: '小明', description: '', images: ['/abs/c.png'],
  createdAt: '2026-10-02T00:00:00Z', updatedAt: '2026-10-02T00:00:00Z',
}

beforeEach(() => {
  vi.mocked(api.rolesList).mockReset()
  vi.mocked(api.rolesList).mockResolvedValue([xiaomei, xiaoming])
})

it('flattens selected roles into absolute reference paths', () => {
  expect(rolesToReferenceImages([xiaomei, xiaoming])).toEqual(['/abs/a.png', '/abs/b.png', '/abs/c.png'])
})

it('toggles ids and stops at max', async () => {
  const onChange = vi.fn()
  const view = render(<RolePicker value={[]} onChange={onChange} max={1} />)
  fireEvent.click(await screen.findByRole('button', { name: '小美' }))
  expect(onChange).toHaveBeenCalledWith(['1'])
  view.rerender(<RolePicker value={['1']} onChange={onChange} max={1} />)
  expect(screen.getByRole('button', { name: '小明' })).toBeDisabled()
  fireEvent.click(screen.getByRole('button', { name: '小美' }))
  expect(onChange).toHaveBeenCalledWith([])
})
