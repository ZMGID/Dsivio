import { useState, useEffect } from 'react'
import { api } from '../../../../api/tauri'
export function AssetImage({ path, name }: { path: string; name: string }) {
  const [src, setSrc] = useState('')
  useEffect(() => {
    let alive = true
    void api
      .workbenchVideoImage(path)
      .then((s) => {
        if (alive) setSrc(s)
      })
      .catch(() => {})
    return () => {
      alive = false
    }
  }, [path])
  return src ? <img src={src} alt={name} /> : <span>{name}</span>
}
