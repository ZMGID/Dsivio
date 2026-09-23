import { beforeEach, describe, expect, it, vi } from 'vitest'
const { parse } = vi.hoisted(() => ({ parse: vi.fn() }))
vi.mock('unified', async importOriginal => {
  const actual = await importOriginal<typeof import('unified')>()
  return { ...actual, unified: () => {
    const processor = actual.unified()
    const original = processor.parse.bind(processor)
    processor.parse = Object.assign((...args: Parameters<typeof original>) => {
      parse()
      return original(...args)
    }, processor.parse)
    return processor
  } }
})
import { referencedArtifactIds } from './artifactReferences'

beforeEach(() => parse.mockClear())

describe('artifact references across virtual row remounts', () => {
  it('does not reparse an unchanged history body when a row remounts', () => {
    const text = '[History](artifact:art_remount)\n\n' + 'Long completed reasoning.\n'.repeat(2000)
    expect([...referencedArtifactIds(text)]).toEqual(['art_remount'])
    expect([...referencedArtifactIds(text)]).toEqual(['art_remount'])
    expect(parse).toHaveBeenCalledOnce()
  })

  it('does not share mutable results or return stale IDs for edited text', () => {
    const text = '[History](artifact:art_original)'
    referencedArtifactIds(text).clear()
    expect([...referencedArtifactIds(text)]).toEqual(['art_original'])
    expect([...referencedArtifactIds(text.replace('art_original', 'art_edited'))]).toEqual(['art_edited'])
  })
})
