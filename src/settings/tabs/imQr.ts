import { toDataURL } from 'qrcode'

/** Renders an authorization URL as a local image. The URL is never sent to an external QR service. */
export async function authorizationQrDataUrl(url: string): Promise<string> {
  const parsed = new URL(url)
  if (parsed.protocol !== 'https:') {
    throw new Error('授权链接无效')
  }
  const dataUrl = await toDataURL(url, { margin: 1, errorCorrectionLevel: 'M', width: 220 })
  if (!dataUrl.startsWith('data:image/')) throw new Error('二维码生成失败')
  return dataUrl
}
