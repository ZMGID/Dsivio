import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { Button } from '../../components/Button'
import { StudioToast } from './StudioToast'

describe('StudioToast', () => {
  it('keeps conflict actions on a floating card instead of a page banner', () => {
    const keep = vi.fn()
    render(
      <StudioToast
        onClose={keep}
        actions={
          <div className="studio-toast-actions">
            <Button size="sm">载入共享版本</Button>
            <Button size="sm" variant="ghost" onClick={keep}>保留本地版本</Button>
          </div>
        }
      >
        聊天或其他页面已修改此草稿。本地编辑已保留，可选择载入共享版本。
      </StudioToast>,
    )
    const toast = screen.getByRole('status')
    expect(toast.className).toContain('studio-toast')
    expect(toast.closest('.is-main')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: '保留本地版本' }))
    fireEvent.click(screen.getByRole('button', { name: '关闭提示' }))
    expect(keep).toHaveBeenCalledTimes(2)
  })
})
