// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { WorkbenchToast } from './WorkbenchPage'

function Owner() {
  const [error, setError] = useState('')
  return (
    <>
      <button onClick={() => setError('缺少商品图')}>trigger</button>
      <WorkbenchToast message={error} duration={5000} onDismiss={() => setError('')} />
    </>
  )
}

describe('WorkbenchToast', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => { cleanup(); vi.useRealTimers() })

  it('shows an error, strips the Error prefix, and disappears after the duration', () => {
    render(<WorkbenchToast message="Error: 保存失败" duration={5000} />)
    expect(screen.getByRole('alert').textContent).toContain('保存失败')
    expect(screen.getByRole('alert').textContent).not.toContain('Error:')
    act(() => { vi.advanceTimersByTime(5000) })
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('shows the same message again when the owner clears it on dismissal and it recurs', () => {
    render(<Owner />)
    fireEvent.click(screen.getByText('trigger'))
    expect(screen.getByRole('alert').textContent).toContain('缺少商品图')
    act(() => { vi.advanceTimersByTime(5000) })
    expect(screen.queryByRole('alert')).toBeNull()
    fireEvent.click(screen.getByText('trigger'))
    expect(screen.getByRole('alert').textContent).toContain('缺少商品图')
  })

  it('notifies the owner when the close button is pressed', () => {
    const onDismiss = vi.fn()
    render(<WorkbenchToast message="失败" onDismiss={onDismiss} />)
    fireEvent.click(screen.getByRole('button', { name: '关闭提示' }))
    expect(onDismiss).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alert')).toBeNull()
  })
})
