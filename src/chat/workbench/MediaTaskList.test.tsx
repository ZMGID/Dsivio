import { fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { save } from '@tauri-apps/plugin-dialog'
import { api } from '../../api/tauri'
import type { MediaTask } from '../../generated/mediaGeneration'
import { MediaTaskRow } from './MediaTaskList'

vi.mock('../../api/tauri', () => ({ api: { openLocalFile: vi.fn(async () => undefined), exportSubtitleFile: vi.fn(async () => '/tmp/out.srt') } }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn(async () => '/tmp/exported.srt') }))

vi.mock('../../components/i18n', () => ({ useLang: () => 'en' }))
vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => path }))
const task: MediaTask = { id: 'speech-1', providerId: 'p', model: 'tts-1', kind: 'speech', status: 'running', createdAt: '2026-10-01T00:00:00Z', error: null, remoteId: null, outputs: [], canResume: false, origin: 'cli/fixture', prompt: '', result: null, requestHash: null, cancellation: null }

describe('Media task cancellation and artifact presentation', () => {
  it('does not claim that an unsupported cancellation stopped a cloud task or refunded it', async () => {
    render(<MediaTaskRow task={task} alt="Speech artifact" onResume={vi.fn()} onError={vi.fn()} onCancel={async () => ({ id: task.id, outcome: 'unsupported', scope: 'none', charged: 'maybe', task })} />)
    await userEvent.click(screen.getByRole('button', { name: 'Cancel task' }))
    expect(screen.getByText('Synthesizing')).toBeInTheDocument()
    expect(screen.getByText(/Cancellation unsupported.*May have been charged/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Cancel task' })).toBeEnabled()
    expect(screen.queryByText('Cancelled')).toBeNull()
  })
  it('keeps cancelled transcription distinct from failure and does not render JSON as video', () => {
    const view = render(<MediaTaskRow task={{ ...task, kind: 'transcribe', status: 'cancelled', outputs: [{ path: '/fixture/evidence.json', mime: 'application/json' }] }} alt="Transcript evidence" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
    expect(screen.getByText('Cancelled')).toBeInTheDocument()
    expect(screen.queryByText('Failed')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Cancel task' })).toBeNull()
    expect(view.container.querySelector('video')).toBeNull()
    expect(screen.getByRole('button', { name: 'Open file' })).toBeInTheDocument()
  })
  it('shows a text record title and plays an edit video without treating subtitles as video', async () => {
    render(<MediaTaskRow task={{ ...task, kind: 'text', status: 'succeeded', prompt: '', result: { title: 'Launch copy' }, outputs: [{ path: '/fixture/output.md', mime: 'text/markdown' }] }} alt="Copy" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
    expect(screen.getByText(/Text ·/)).toBeInTheDocument()
    expect(screen.getByText('Launch copy')).toBeInTheDocument()
    const edit = render(<MediaTaskRow task={{ ...task, kind: 'edit', status: 'succeeded', outputs: [{ path: '/fixture/out.mp4', mime: 'video/mp4' }, { path: '/fixture/cues.srt', mime: 'application/x-subrip' }] }} alt="Edit" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
    expect(edit.container.querySelectorAll('video')).toHaveLength(1)
    expect(edit.container.querySelector('video')).toHaveAttribute('src', '/fixture/out.mp4')
    const row = within(edit.container)
    expect(row.getAllByRole('button', { name: 'Open file' })).toHaveLength(2)
    await userEvent.click(row.getByRole('button', { name: 'Export subtitles' }))
    expect(save).toHaveBeenCalled()
    expect(api.exportSubtitleFile).toHaveBeenCalledWith('/fixture/cues.srt', '/tmp/exported.srt')
    await userEvent.click(row.getAllByRole('button', { name: 'Open file' })[1])
    expect(api.openLocalFile).toHaveBeenCalledWith('/fixture/cues.srt')
  })
  it('plays a saved speech artifact with audio controls instead of a video element', () => {
    const view = render(<MediaTaskRow task={{ ...task, status: 'succeeded', outputs: [{ path: '/fixture/speech.wav', mime: 'audio/wav' }] }} alt="Speech artifact" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
    expect(view.container.querySelector('audio')).toHaveAttribute('controls')
    expect(view.container.querySelector('audio')).toHaveAttribute('src', '/fixture/speech.wav')
    expect(view.container.querySelector('video')).toBeNull()
  })
})

it('decodes a first video frame without autoplay and does not reset a started video', () => {
  const view = render(<MediaTaskRow task={{ ...task, kind: 'video', status: 'succeeded', outputs: [{ path: '/fixture/out.mp4', mime: 'video/mp4' }] }} alt="Video" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
  const video = view.container.querySelector('video')!
  Object.defineProperty(video, 'duration', { configurable: true, value: 2 })
  fireEvent.loadedMetadata(video)
  expect(video.currentTime).toBe(0.001)
  expect(video.autoplay).toBe(false)
  video.currentTime = 1
  fireEvent.loadedMetadata(video)
  expect(video.currentTime).toBe(1)
})
