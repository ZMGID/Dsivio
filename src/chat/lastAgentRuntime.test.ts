import { describe, expect, it } from 'vitest'
import { parseLastAgentRuntime } from './lastAgentRuntime'
describe('last runtime migration', () => {
  it('drops remembered CLI models and starts new drafts with Dsivio Agent', () => {
    expect(parseLastAgentRuntime({kind: 'external', externalAgentId: 'codex', externalModel: 'old'})).toMatchObject({kind: 'builtin', externalAgentId: null, externalModel: null})
  })
  it('migrates Chat to Agent', () => expect(parseLastAgentRuntime({kind: 'chat'})?.kind).toBe('builtin'))
  it('rejects invalid preferences', () => expect(parseLastAgentRuntime(null)).toBeNull())
})
