import { describe, expect, it } from 'vitest'
import { i18n } from '../../../components/i18n'
import { canConnectPorts, imageReplicaTemplate, officialTemplates, paletteEntry } from './workflowCatalog'

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
    expect(officialTemplates(i18n.zh)).toHaveLength(1)
  })

  it('only connects matching port kinds', () => {
    expect(canConnectPorts('image', 'image')).toBe(true)
    expect(canConnectPorts('text', 'image')).toBe(false)
    expect(paletteEntry('image.understand')?.outputs[0]?.kind).toBe('text')
    expect(paletteEntry('image.generate')?.inputs.map((port) => port.kind)).toEqual(['text', 'image'])
  })
})
