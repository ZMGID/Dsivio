import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { MediaTask } from '../../generated/mediaGeneration'
import { MediaTaskRow } from './MediaTaskList'

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
  it('plays a saved speech artifact with audio controls instead of a video element', () => {
    const view = render(<MediaTaskRow task={{ ...task, status: 'succeeded', outputs: [{ path: '/fixture/speech.wav', mime: 'audio/wav' }] }} alt="Speech artifact" onResume={vi.fn()} onError={vi.fn()} onCancel={vi.fn()} />)
    expect(view.container.querySelector('audio')).toHaveAttribute('controls')
    expect(view.container.querySelector('audio')).toHaveAttribute('src', '/fixture/speech.wav')
    expect(view.container.querySelector('video')).toBeNull()
  })
})
