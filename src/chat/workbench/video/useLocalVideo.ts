import { useEffect, useRef, useState } from 'react'

export type LocalVideo = { id: string; name: string; url: string }

export function revokeVideo(file: LocalVideo | null): void {
  if (file) URL.revokeObjectURL(file.url)
}

export function useLocalVideo(): [LocalVideo | null, (file: LocalVideo | null) => void] {
  const [file, setFile] = useState<LocalVideo | null>(null)
  const ref = useRef(file)
  ref.current = file
  useEffect(() => () => revokeVideo(ref.current), [])
  return [file, setFile]
}
