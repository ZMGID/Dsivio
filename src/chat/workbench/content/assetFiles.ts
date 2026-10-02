import { convertFileSrc } from '@tauri-apps/api/core'

/** Read a library file the asset protocol is allowed to serve (media-tasks). */
export async function readAssetText(path: string): Promise<string> {
  const response = await fetch(convertFileSrc(path))
  if (!response.ok) throw new Error(`无法读取文案（${response.status}）`)
  return response.text()
}
