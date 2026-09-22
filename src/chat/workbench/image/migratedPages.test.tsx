import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, expect, it, vi } from 'vitest'
import { api } from '../../../api/tauri'
import { PosterPage } from './PosterPage'
import { RetouchPage } from './RetouchPage'
import { MigratePage } from './MigratePage'
import { DressPage } from './DressPage'
import { EditImagePage } from './EditImagePage'
import { i18n } from '../../../components/i18n'
import type { ReactNode } from 'react'
vi.mock('../WorkbenchMediaModelSelect', () => ({ WorkbenchMediaModelSelect: ({ render }: { render: (control: ReactNode, provider: { id: string }, model: string) => ReactNode }) => render(<div>图片模型</div>, { id: 'image-provider' }, 'image-model') }))
vi.mock('../copy/CopyUploadField', () => ({ CopyUploadField: ({ label, onChange }: { label: string, onChange: (v: { id: string, name: string, url: string }[]) => void }) => <button onClick={() => onChange([{ id: label, name: label, url: 'blob:input' }])}>{label}</button> }))
beforeEach(() => {
  vi.restoreAllMocks()
  vi.spyOn(api, 'listMediaTasks').mockResolvedValue([])
  vi.spyOn(api, 'startMediaGeneration').mockResolvedValue({ id: 'result', providerId: 'image-provider', model: 'image-model', kind: 'image', status: 'running', createdAt: '', error: null, remoteId: null, outputs: [], canResume: false, origin: null, prompt: '' })
  vi.stubGlobal('fetch', vi.fn(async () => ({ blob: async () => new Blob(['image'], { type: 'image/png' }) })))
  URL.revokeObjectURL = vi.fn()
})
const t = i18n.zh
it.each([
  { Page: PosterPage, origin: 'poster', uploads: [], placeholder: t.workbenchPosterBriefHint, action: t.workbenchPosterGenerate },
  { Page: RetouchPage, origin: 'retouch', uploads: [t.workbenchRetouchUpload], placeholder: '', action: t.workbenchRetouchGenerate },
  { Page: MigratePage, origin: 'migrate', uploads: [t.workbenchMigrateProduct, t.workbenchImageRef], placeholder: '', action: t.workbenchMigrateGenerate.replace('{n}', '1') },
  { Page: DressPage, origin: 'dress', uploads: [t.workbenchDressProduct, t.workbenchDressRef], placeholder: '', action: t.workbenchDressGenerate.replace('{n}', '1') },
  { Page: EditImagePage, origin: 'edit', uploads: [t.workbenchEditUpload], placeholder: t.workbenchEditBriefHint, action: t.workbenchEditGenerate },
])('$origin submits user input and lists shared history', async ({ Page, origin, uploads, placeholder, action }) => {
  render(<Page />)
  for (const name of uploads) fireEvent.click(screen.getByRole('button', { name }))
  if (placeholder) fireEvent.change(screen.getByPlaceholderText(placeholder), { target: { value: '保留品牌，调整光线' } })
  fireEvent.click(screen.getByRole('button', { name: action }))
  await waitFor(() => expect(api.startMediaGeneration).toHaveBeenCalledTimes(1))
  expect(api.startMediaGeneration).toHaveBeenCalledWith(expect.objectContaining({ providerId: 'image-provider', model: 'image-model', kind: 'image', origin: `workbench/${origin}`, images: expect.any(Array) }))
  expect(api.listMediaTasks).toHaveBeenCalledWith({ origin: `workbench/${origin}` })
})
it('does not submit edits without an image', () => {
  render(<EditImagePage />)
  fireEvent.click(screen.getByRole('button', { name: t.workbenchEditGenerate }))
  expect(api.startMediaGeneration).not.toHaveBeenCalled()
  expect(screen.getByText(t.workbenchImageNeedProduct)).toBeVisible()
})
