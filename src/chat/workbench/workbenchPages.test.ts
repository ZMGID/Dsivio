import { describe, expect, it } from 'vitest'
import {
  isWorkbenchSubpage,
  workbenchHash,
  workbenchNavItem,
  workbenchPageFromPath,
  workbenchWorkflowHash,
  workbenchWorkflowIdFromPath,
} from './workbenchPages'

describe('workbenchPages', () => {
  it('maps home and commerce subpages to workbench hashes', () => {
    expect(workbenchHash('home')).toBe('#chat/workbench')
    expect(workbenchHash('shops')).toBe('#chat/workbench/shops')
    expect(workbenchNavItem('home')).toBe('workbench')
    expect(workbenchNavItem('listing')).toBe('workbench/listing')
  })

  it('reads the page id from a workbench path and ignores unknown suffixes', () => {
    expect(workbenchPageFromPath('chat/workbench')).toBe('home')
    expect(workbenchPageFromPath('chat/workbench/shops')).toBe('shops')
    expect(workbenchPageFromPath('chat/workbench/workflows/extra')).toBe('workflows')
    expect(workbenchPageFromPath('chat/workbench/chatgpt')).toBe('home')
    expect(isWorkbenchSubpage('shops')).toBe(true)
    expect(isWorkbenchSubpage('chatgpt')).toBe(false)
    expect(workbenchPageFromPath('chat/workbench/ranks')).toBe('ranks')
    expect(workbenchPageFromPath('chat/workbench/match')).toBe('match')
    expect(workbenchPageFromPath('chat/workbench/picks')).toBe('picks')
  })

  it('keeps a generation workflow id on the workbench/workflows hash', () => {
    expect(workbenchWorkflowHash('ab/c')).toBe('#chat/workbench/workflows/ab%2Fc')
    expect(workbenchWorkflowIdFromPath('chat/workbench/workflows/ab%2Fc')).toBe('ab/c')
    expect(workbenchWorkflowIdFromPath('chat/workbench/workflows')).toBe(null)
  })
})
