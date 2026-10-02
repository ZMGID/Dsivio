import { expect, it } from 'vitest'
import type { MediaTask } from '../../../generated/mediaGeneration'
import { assetOrigins, assetTitle, filterAssets } from './assetLibrary'

function task(partial: Partial<MediaTask>): MediaTask {
  return {
    id: 'task-1',
    providerId: 'local',
    model: 'record',
    kind: 'image',
    status: 'succeeded',
    createdAt: '2026-10-02T00:00:00Z',
    error: null,
    remoteId: null,
    outputs: [{ path: '/tmp/task-1.png', mime: 'image/png' }],
    canResume: false,
    origin: 'workbench/main',
    prompt: '海报',
    result: { title: '海报' },
    requestHash: null,
    cancellation: null,
    ...partial,
  }
}

const tasks = [
  task({ id: 'img', kind: 'image', origin: 'workbench/main', result: { title: '主图' }, prompt: '白底' }),
  task({ id: 'vid', kind: 'video', origin: 'chat', result: { title: '口播' }, prompt: '介绍', outputs: [{ path: '/tmp/a.mp4', mime: 'video/mp4' }] }),
  task({ id: 'note', kind: 'text', origin: 'workbench/articles', model: 'record', result: { title: '通勤包' }, prompt: '帆布', outputs: [{ path: '/tmp/output.md', mime: 'text/markdown' }] }),
  task({ id: 'run', status: 'running', result: { title: '还在跑' } }),
  task({ id: 'bad', status: 'failed', outputs: [], result: { title: '失败' } }),
]

it('keeps only completed outputs and filters kind, origin, and keyword', () => {
  expect(filterAssets(tasks, { kind: 'all', origin: '', keyword: '' }).map((item) => item.id)).toEqual(['img', 'vid', 'note'])
  expect(filterAssets(tasks, { kind: 'text', origin: '', keyword: '' }).map((item) => item.id)).toEqual(['note'])
  expect(filterAssets(tasks, { kind: 'all', origin: 'chat', keyword: '' }).map((item) => item.id)).toEqual(['vid'])
  expect(filterAssets(tasks, { kind: 'all', origin: '', keyword: '帆布' }).map((item) => item.id)).toEqual(['note'])
  expect(filterAssets(tasks, { kind: 'image', origin: 'workbench/main', keyword: '主图' }).map((item) => item.id)).toEqual(['img'])
  expect(filterAssets(tasks, { kind: 'all', origin: '', keyword: 'output.md' }).map((item) => item.id)).toEqual(['note'])
})

it('reads a stored title and lists origins from completed tasks', () => {
  expect(assetTitle(tasks[2])).toBe('通勤包')
  expect(assetTitle(task({ result: null, prompt: '  ', outputs: [{ path: '/tmp/folder/plain.png', mime: 'image/png' }] }))).toBe('plain.png')
  expect(assetOrigins(tasks)).toEqual(['chat', 'workbench/articles', 'workbench/main'])
})
