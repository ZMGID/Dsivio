const PREFIXES = [
  '优化后的问题：',
  '优化后的提问：',
  '优化后：',
  'Rewritten question:',
  'Rewritten:',
  'Optimized question:',
  'Optimized:',
]

/** Drop a wrapping code fence, a leading label, and surrounding quotes. */
export function stripFences(raw: string): string {
  let text = raw.trim()
  if (!text) return ''
  if (text.startsWith('```')) {
    const lines = text.split('\n')
    if (lines[0]?.startsWith('```')) lines.shift()
    if (lines.at(-1)?.trim() === '```') lines.pop()
    text = lines.join('\n').trim()
  }
  for (const prefix of PREFIXES) {
    if (text.startsWith(prefix)) text = text.slice(prefix.length).trim()
  }
  return text.replace(/^["'`“”‘’]+|["'`“”‘’]+$/g, '').trim()
}
