import { describe, expect, it } from 'vitest'
import { connectionProblem, graphProblems } from './workflowGraph'
import { i18n } from '../../../components/i18n'
import { imageReplicaTemplate, officialTemplates, paletteEntry } from './workflowCatalog'

describe('workflowCatalog', () => {
  it('maps the live image-replica graph: 5 nodes and 4 typed edges', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    expect(flow.nodes.map((node) => node.kind)).toEqual([
      'image.upload',
      'image.understand',
      'image.upload',
      'image.generate',
      'image.download',
    ])
    expect(flow.edges).toHaveLength(4)
    expect(flow.nodes[0]?.title).toBe('参考图')
    expect(flow.nodes[2]?.title).toBe('产品图')
    expect(officialTemplates(i18n.zh)).toHaveLength(2)
  })

  it('only connects matching port kinds', () => {
    const flow = imageReplicaTemplate(i18n.zh)
    const open = { ...flow, edges: [] }
    expect(connectionProblem(open, { source: flow.nodes[0].id, target: flow.nodes[1].id, sourceHandle: 'image', targetHandle: 'image' })).toBeNull()
    expect(connectionProblem(open, { source: flow.nodes[1].id, target: flow.nodes[3].id, sourceHandle: 'text', targetHandle: 'image' })).toBe('端口类型不匹配：text不能接到image')
    expect(paletteEntry('image.understand')?.outputs[0]?.kind).toBe('text')
    expect(paletteEntry('image.generate')?.inputs.map((port) => port.kind)).toEqual(['text', 'image'])
  })
})

it('offers a runnable local example without assets or model configuration', () => {
  const flow = officialTemplates(i18n.zh).find(t => t.id === 'text-demo')!.build()
  expect(graphProblems(flow)).toEqual([])
  expect(flow.nodes.map(n => n.kind)).toEqual(['prompt.input', 'text.join', 'text.preview'])
})
