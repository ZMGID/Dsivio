import { useEffect, useRef } from 'react'

// Reading a destination must not disable the whole workspace. Only the latest
// destination may apply its result; repeated clicks on the same pending row coalesce.
export function useStudioNavigation() {
  const version = useRef(0)
  const pending = useRef<string>()
  const cancel = () => { version.current++; pending.current = undefined }
  useEffect(() => () => { version.current++; pending.current = undefined }, [])
  const open = async (id: string, read: (current: () => boolean) => Promise<void>, onError: (error: unknown) => void) => {
    if (pending.current === id) return
    const ticket = ++version.current
    pending.current = id
    const current = () => ticket === version.current
    try { await read(current) }
    catch (error) { if (current()) onError(error) }
    finally { if (current()) pending.current = undefined }
  }
  return { open, cancel }
}
