import { useEffect, useRef, useState } from 'react'
import { revokeImages, type LocalImage } from '../localMedia'

export function useLocalImages(): [LocalImage[], (files: LocalImage[]) => void] {
  const [files, setFiles] = useState<LocalImage[]>([])
  const ref = useRef(files)
  ref.current = files
  useEffect(() => () => revokeImages(ref.current), [])
  return [files, setFiles]
}
