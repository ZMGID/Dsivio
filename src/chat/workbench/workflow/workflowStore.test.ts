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
