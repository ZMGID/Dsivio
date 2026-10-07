/** @vitest-environment jsdom */
import { afterEach, describe, expect, it } from 'vitest'
import { blankWorkflow } from './workflowModel'
import { parseWorkflow, workflowStore } from './workflowStore'

describe('workflowStore', () => {
  afterEach(() => {
    globalThis.localStorage.removeItem('kivio.workbench.workflows')
  })

  it('round-trips a generation workflow and rejects automation-shaped json', () => {
    const saved = workflowStore.save(blankWorkflow('图片复刻工作流'))
    expect(workflowStore.get(saved.id)?.name).toBe('图片复刻工作流')
    expect(parseWorkflow({ id: 'x', name: 'n', nodes: [], edges: [] })?.id).toBe('x')
    expect(parseWorkflow({ id: 'auto', name: 'n', enabled: true, nodes: [{ type: 'trigger.manual' }] })).toBe(null)
  })
})

it('refuses to overwrite an unreadable workflow list', () => {
  localStorage.setItem('kivio.workbench.workflows', '{')
  const failed = blankWorkflow('x')
  expect(() => workflowStore.save(failed)).toThrow(/损坏/)
  expect(localStorage.getItem('kivio.workbench.workflows')).toBe('{')
  expect(() => workflowStore.remove('x')).toThrow(/损坏/)
  expect(localStorage.getItem('kivio.workbench.workflows')).toBe('{')
  localStorage.setItem('kivio.workbench.workflows', '{"id":"x"}')
  const failedAgain = blankWorkflow('y')
  expect(() => workflowStore.save(failedAgain)).toThrow(/损坏/)
  expect(localStorage.getItem('kivio.workbench.workflows')).toBe('{"id":"x"}')
  localStorage.setItem('kivio.workbench.workflows', '[]')
  workflowStore.remove(failed.id)
  workflowStore.remove(failedAgain.id)
})

it('keeps an unparseable entry when saving another draft', () => {
  const kept = blankWorkflow('kept')
  const broken = { broken: true }
  localStorage.setItem('kivio.workbench.workflows', JSON.stringify([broken, kept]))
  const saved = workflowStore.save(blankWorkflow('added'))
  const stored = JSON.parse(localStorage.getItem('kivio.workbench.workflows')!) as unknown[]
  expect(stored).toContainEqual(broken)
  expect(stored).toContainEqual(kept)
  expect(stored).toContainEqual(expect.objectContaining({ id: saved.id, name: 'added' }))
  expect(workflowStore.list().map(item => item.id).sort()).toEqual([kept.id, saved.id].sort())
  expect(workflowStore.get(kept.id)?.name).toBe('kept')
  workflowStore.remove(kept.id)
  const afterRemove = JSON.parse(localStorage.getItem('kivio.workbench.workflows')!) as unknown[]
  expect(afterRemove).toContainEqual(broken)
  expect(afterRemove.some(item => typeof item === 'object' && item !== null && 'id' in item && item.id === kept.id)).toBe(false)
})

it('reads old input drafts with defaults and preserves stored replica configurations after reopening', () => {
  const flow = blankWorkflow('legacy')
  flow.nodes = [{ id: 'legacy-prompt', kind: 'prompt.input', title: 'prompt', position: { x: 0, y: 0 } }]
  localStorage.setItem('kivio.workbench.workflows', JSON.stringify([flow]))
  expect(workflowStore.get(flow.id)?.nodes[0].config).toEqual({ type: 'prompt', text: '' })
})
