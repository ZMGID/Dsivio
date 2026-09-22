import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ProductModeSwitcher } from './ProductModeSwitcher'

describe('ProductModeSwitcher', () => {
  it('switches to Workbench on click without opening a menu', async () => {
    const onSelect = vi.fn()
    render(<ProductModeSwitcher mode="chat" onSelect={onSelect} />)

    expect(screen.queryByRole('menu')).toBeNull()
    expect(screen.getByText('Dsivio')).toBeInTheDocument()

    await userEvent.click(screen.getByRole('button', { name: '切换到 Workbench' }))

    expect(onSelect).toHaveBeenCalledTimes(1)
    expect(onSelect).toHaveBeenCalledWith('workbench')
  })

  it('switches back to Dsivio on the next click', async () => {
    const onSelect = vi.fn()
    render(<ProductModeSwitcher mode="workbench" onSelect={onSelect} />)

    await userEvent.click(screen.getByRole('button', { name: '切换到 Dsivio' }))

    expect(onSelect).toHaveBeenCalledWith('chat')
  })
})
