import { act, renderHook } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { useStudioNavigation } from './useStudioNavigation'

it('coalesces duplicate clicks and only applies the latest destination', async () => {
  const { result } = renderHook(useStudioNavigation)
  const apply = vi.fn()
  const error = vi.fn()
  let first!: () => void
  let second!: () => void
  const slow = vi.fn(async (current: () => boolean) => {
    await new Promise<void>(resolve => { first = resolve })
    if (current()) apply('first')
  })
  const a = result.current.open('first', slow, error)
  await result.current.open('first', slow, error)
  const b = result.current.open('second', async current => {
    await new Promise<void>(resolve => { second = resolve })
    if (current()) apply('second')
  }, error)
  await act(async () => { second(); await b; first(); await a })
  expect(slow).toHaveBeenCalledTimes(1)
  expect(apply.mock.calls).toEqual([['second']])
  expect(error).not.toHaveBeenCalled()
})

it('discards errors from abandoned or unmounted reads', async () => {
  const { result, unmount } = renderHook(useStudioNavigation)
  const error = vi.fn()
  let reject!: (error: Error) => void
  const read = () => new Promise<void>((_resolve, fail) => { reject = fail })
  const a = result.current.open('first', read, error)
  result.current.cancel()
  await act(async () => { reject(new Error('old destination')); await a })
  const b = result.current.open('second', read, error)
  unmount()
  await act(async () => { reject(new Error('unmounted')); await b })
  expect(error).not.toHaveBeenCalled()
})
