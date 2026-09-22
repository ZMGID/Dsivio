/** 本机选中的一张图：对象 URL 只在页面存活期间有效。 */
export type LocalImage = { id: string; name: string; url: string }

export function revokeImages(items: LocalImage[]): void {
  for (const item of items) URL.revokeObjectURL(item.url)
}

/** 把本机选的文件读成 data URL；生成请求里的参考图都走这一种编码。 */
export const dataUrl = (blob: Blob) => new Promise<string>((resolve, reject) => {
  const reader = new FileReader()
  reader.onload = () => resolve(String(reader.result))
  reader.onerror = () => reject(new Error('读取图片失败'))
  reader.readAsDataURL(blob)
})

export async function readImages(images: LocalImage[]): Promise<string[]> {
  return Promise.all(images.map(async (image) => dataUrl(await (await fetch(image.url)).blob())))
}
