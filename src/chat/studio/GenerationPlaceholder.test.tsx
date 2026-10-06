import { render, screen } from '@testing-library/react'
import { expect, it } from 'vitest'
import { GenerationPlaceholder } from './GenerationPlaceholder'
it('uses the requested canvas ratio and safely falls back for automatic sizes', () => {
  const { rerender } = render(<GenerationPlaceholder ratio="16:9" label="正在生成" />)
  expect(screen.getByRole('status').style.aspectRatio).toBe(String(16 / 9))
  rerender(<GenerationPlaceholder ratio="auto" label="正在生成" />)
  expect(screen.getByRole('status').style.aspectRatio).toBe('1')
})
