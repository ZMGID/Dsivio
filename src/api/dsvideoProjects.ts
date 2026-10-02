import { invoke } from '@tauri-apps/api/core'

export type DsvideoProject = { id: string; name: string; path: string; initializedAt: number; lastUsedAt: number; available: boolean }
export type DsvideoRegistry = { version: number; current: string | null; projects: DsvideoProject[]; document: string }
export type DsvideoProjectContext = { id: string; name: string; rootPath: string }
export const dsvideoProjectsApi = {
  list: () => invoke<DsvideoRegistry>('dsvideo_projects', { action: 'list', path: null, name: null }),
  init: (path: string, name: string) => invoke<DsvideoRegistry>('dsvideo_projects', { action: 'init', path, name }),
  remove: (path: string) => invoke<DsvideoRegistry>('dsvideo_projects', { action: 'remove', path, name: null }),
  bind: (path: string) => invoke<DsvideoProjectContext>('dsvideo_project_bind', { path }),
}
