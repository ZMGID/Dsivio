/**
 * Renders an authorization URL as a local image.
 * The URL must never be sent to an external QR service.
 * Parent needs the `qrcode` dependency (and `@types/qrcode`) for this import.
 */
export async function authorizationQrDataUrl(url: string): Promise<string> {
  const parsed = new URL(url)
  if (parsed.protocol !== 'https:' && parsed.protocol !== 'http:') {
    throw new Error('授权链接无效')
  }
  const qrcode = await import('qrcode') as {
    toDataURL: (text: string, options?: { margin?: number, errorCorrectionLevel?: 'L' | 'M' | 'Q' | 'H', width?: number }) => Promise<string>
  }
  const dataUrl = await qrcode.toDataURL(url, { margin: 1, errorCorrectionLevel: 'M', width: 220 })
  if (!dataUrl.startsWith('data:image/')) throw new Error('二维码生成失败')
  return dataUrl
}
