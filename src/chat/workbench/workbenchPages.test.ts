import { describe, expect, it } from 'vitest'
import {
  WORKBENCH_NAV,
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
    expect(workbenchPageFromPath('chat/workbench/posts')).toBe('posts')
    expect(workbenchPageFromPath('chat/workbench/articles')).toBe('articles')
    expect(workbenchPageFromPath('chat/workbench/main')).toBe('main')
    expect(workbenchPageFromPath('chat/workbench/edit')).toBe('edit')
    expect(workbenchPageFromPath('chat/workbench/shorts')).toBe('shorts')
    expect(workbenchPageFromPath('chat/workbench/subs')).toBe('subs')
    expect(workbenchPageFromPath('chat/workbench/publish')).toBe('publish')
    expect(workbenchPageFromPath('chat/workbench/pdata')).toBe('pdata')
    expect(workbenchPageFromPath('chat/workbench/roles')).toBe('roles')
    expect(workbenchPageFromPath('chat/workbench/usage')).toBe('usage')
  })

  it('keeps content and stats after publish', () => {
    expect(WORKBENCH_NAV.groups.map((group) => group.id)).toEqual([
      'commerce', 'sourcing', 'copy', 'image', 'video', 'publish', 'content', 'stats',
    ])
    expect(WORKBENCH_NAV.groups.at(-2)?.entries.map((entry) => entry.page)).toEqual(['template-builder', 'image-templates', 'video-templates', 'roles', 'assets'])
    expect(WORKBENCH_NAV.groups.at(-1)?.entries.map((entry) => entry.page)).toEqual(['usage'])
  })

  it('keeps a generation workflow id on the workbench/workflows hash', () => {
    expect(workbenchWorkflowHash('ab/c')).toBe('#chat/workbench/workflows/ab%2Fc')
    expect(workbenchWorkflowIdFromPath('chat/workbench/workflows/ab%2Fc')).toBe('ab/c')
    expect(workbenchWorkflowIdFromPath('chat/workbench/workflows')).toBe(null)
  })
})
