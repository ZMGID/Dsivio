import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { DocumentProcessingPanel } from './DocumentProcessingPanel'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('document OCR platform options', () => {
  it('keeps RapidOCR but removes system OCR on macOS', () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Macintosh; Intel Mac OS X)')
    render(<DocumentProcessingPanel lang="zh" onChange={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: '关闭' }))
    expect(screen.queryByRole('option', { name: '系统 OCR' })).not.toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'RapidOCR 离线' })).toBeInTheDocument()
  })

  it('retains system OCR on Windows', () => {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
    render(<DocumentProcessingPanel lang="zh" onChange={vi.fn()} />)
    fireEvent.click(screen.getByRole('button', { name: '关闭' }))
    expect(screen.getByRole('option', { name: '系统 OCR' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'RapidOCR 离线' })).toBeInTheDocument()
  })
})
