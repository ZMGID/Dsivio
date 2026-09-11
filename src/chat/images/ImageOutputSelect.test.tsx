import { expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { ImageRatioSelect, ImageResolutionSelect } from './ImageOutputSelect'

it('keeps ratio and resolution as two fields and snaps resolution when the ratio changes', () => {
  const onChange = vi.fn()
  const { rerender } = render(
    <ImageRatioSelect ratio="1:1" resolution="1k" model="gpt-image-2" protocol="async" onChange={onChange} />,
  )
  fireEvent.click(screen.getByRole('button', { name: '比例' }))
  expect(screen.getByRole('option', { name: '1:1 正方形' })).toBeInTheDocument()
  expect(screen.getByRole('option', { name: '9:16 竖版' })).toBeInTheDocument()
  fireEvent.click(screen.getByRole('option', { name: '16:9 横版' }))
  expect(onChange).toHaveBeenCalledWith({ ratio: '16:9', resolution: '1k' })

  rerender(<ImageResolutionSelect ratio="16:9" resolution="2k" model="gpt-image-2" protocol="async" onChange={onChange} />)
  fireEvent.click(screen.getByRole('button', { name: '分辨率' }))
  fireEvent.click(screen.getByRole('option', { name: '4K · 3840×2160' }))
  expect(onChange).toHaveBeenCalledWith({ ratio: '16:9', resolution: '4k' })
})
