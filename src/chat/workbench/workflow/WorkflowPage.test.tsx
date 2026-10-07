/** @vitest-environment jsdom */
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { i18n } from '../../../components/i18n'
import { WorkflowPage } from './WorkflowPage'
import { imageReplicaTemplate } from './workflowCatalog'
import { workflowStore } from './workflowStore'
import { blankWorkflow, type GenerationWorkflow } from './workflowModel'
vi.mock('./WorkflowCanvas', () => ({ WorkflowCanvas: ({ flow }: { flow: GenerationWorkflow }) => <output data-testid="opened">{JSON.stringify(flow)}</output> }))
vi.mock('../../../components/i18n', async () => {
  const actual = await vi.importActual<typeof import('../../../components/i18n')>('../../../components/i18n')
  return { ...actual, useT: () => actual.i18n.zh }
})
afterEach(() => { localStorage.clear(); history.replaceState(null, '', '#chat/workbench/workflows'); vi.restoreAllMocks(); vi.unstubAllGlobals() })
it('exports a configured replica and imports it as a new editable draft with every parameter intact', async () => {
  history.replaceState(null, '', '#chat/workbench/workflows')
  const flow = imageReplicaTemplate(i18n.zh)
  flow.name = '已配置复刻'; flow.nodes[1].note = '保留布局'
  flow.nodes[0].config = { type: 'assets', assets: [{ path: '/local/reference.png', name: 'reference', description: '布光参考' }] }
  flow.nodes[3].config = { type: 'generate', model: { providerId: 'p1', model: 'gpt-image-1' }, assets: [], prompt: '备用本地词', options: { output: '1:1:1k' } }
  const saved = workflowStore.save(flow)
  let exported!: Blob
  vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: (blob: Blob) => { exported = blob; return 'blob:export' }, revokeObjectURL: vi.fn() }))
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  const view = render(<WorkflowPage />)
  fireEvent.click(screen.getByText(i18n.zh.wfExport))
  const text = await new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = reject; reader.readAsText(exported) })
  expect(JSON.parse(text)).toEqual(saved)
  const input = view.container.querySelector('input[type="file"]')!
  await act(async () => { fireEvent.change(input, { target: { files: [{ text: async () => text }] } }) })
  await waitFor(() => expect(screen.getByTestId('opened')).toBeVisible())
  const reopened = JSON.parse(screen.getByTestId('opened').textContent!)
  expect(reopened.id).not.toBe(flow.id)
  expect(reopened.nodes).toEqual(flow.nodes); expect(reopened.edges).toEqual(flow.edges)
  expect(workflowStore.list()).toHaveLength(2)
})

it('restores stacked deletions from newest to oldest', () => {
  const first = workflowStore.save(blankWorkflow('流程A'))
  const second = workflowStore.save(blankWorkflow('流程B'))
  render(<WorkflowPage />)
  const remove = (name: string) => {
    const card = screen.getByRole('heading', { name }).closest('article')
    if (!card) throw new Error('missing card')
    fireEvent.click(within(card).getByRole('button', { name: '删除' }))
  }
  remove('流程A')
  remove('流程B')
  expect(workflowStore.get(first.id)).toBeNull()
  expect(workflowStore.get(second.id)).toBeNull()
  expect(screen.getByText(/2 个可撤销/)).toBeVisible()
  fireEvent.click(screen.getByRole('button', { name: '撤销删除' }))
  expect(workflowStore.get(second.id)?.name).toBe('流程B')
  expect(workflowStore.get(first.id)).toBeNull()
  fireEvent.click(screen.getByRole('button', { name: '撤销删除' }))
  expect(workflowStore.get(first.id)?.name).toBe('流程A')
  expect(workflowStore.get(second.id)?.name).toBe('流程B')
  expect(screen.queryByRole('button', { name: '撤销删除' })).toBeNull()
})
