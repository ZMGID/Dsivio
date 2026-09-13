import type { LensReplaceGroup, LensReplaceRenderSlot } from '../api/tauri'

export type TextBounds = { width: number; height: number }

export type TextMeasure = (text: string, fontPx: number) => number | TextBounds

function measuredBounds(measure: TextMeasure, text: string, fontPx: number): TextBounds {
  const result = measure(text, fontPx)
  return typeof result === 'number' ? { width: result, height: fontPx * 1.18 } : result
}

export type ReplaceTextFlowSlot = TextBounds

/** Space is measured from the same ink anchor used by the Canvas renderer. */
export function replaceSlotTextBounds(slot: LensReplaceRenderSlot, padding: number): TextBounds {
  const { bounds } = slot
  return {
    width: Math.max(1, slot.align === 'left'
      ? bounds.x + bounds.width - slot.anchor.x - padding
      : bounds.width - padding * 2),
    height: Math.max(1, slot.flow === 'exact_line' || slot.verticalAlign === 'top'
      ? bounds.y + bounds.height - slot.anchor.y - padding
      : bounds.height - padding * 2),
  }
}

export type ReplaceTextFlowSlotLayout = {
  lines: string[]
  contentWidth: number
  contentHeight: number
}

export type ReplaceTextFlowLayout = {
  fontPx: number
  lineHeight: number
  safeScale: number
  slots: ReplaceTextFlowSlotLayout[]
  complete: boolean
}

export type ReplaceRegionKind = 'cell' | 'line' | 'paragraph' | 'heading'

export function replaceTextVerticalOffset(
  kind: ReplaceRegionKind,
  availableHeight: number,
  contentHeight: number,
): number {
  if (kind === 'paragraph') return 0
  return Math.max(0, (availableHeight - contentHeight) / 2)
}

const CJK = /[\u2e80-\u9fff\uf900-\ufaff\u3040-\u30ff\uac00-\ud7af]/

export function tokenizeReplaceText(text: string): string[] {
  const tokens: string[] = []
  let latin = ''
  const flushLatin = () => {
    if (latin) tokens.push(latin)
    latin = ''
  }
  for (const char of text) {
    if (char === '\n') {
      flushLatin()
      tokens.push('\n')
    } else if (CJK.test(char)) {
      flushLatin()
      tokens.push(char)
    } else if (/\s/.test(char)) {
      flushLatin()
      tokens.push(char)
    } else {
      latin += char
    }
  }
  flushLatin()
  return tokens
}

function takeReplaceFlowLine(
  tokens: string[],
  maxWidth: number,
  fontPx: number,
  measure: (text: string, fontPx: number) => number,
): string {
  let current = ''
  while (tokens.length > 0) {
    const token = tokens[0]
    if (token === '\n') {
      tokens.shift()
      break
    }
    if (!current && /^\s+$/.test(token)) {
      tokens.shift()
      continue
    }
    const candidate = current + token
    if (!current || measure(candidate, fontPx) <= maxWidth) {
      if (measure(candidate, fontPx) <= maxWidth) {
        current = candidate
        tokens.shift()
        continue
      }
    }
    if (current) break

    let prefix = ''
    let consumed = 0
    for (const char of token) {
      const next = prefix + char
      if (prefix && measure(next, fontPx) > maxWidth) break
      prefix = next
      consumed += char.length
    }
    current = prefix
    const remainder = token.slice(consumed)
    if (remainder) tokens[0] = remainder
    else tokens.shift()
    break
  }
  return current.trimEnd()
}

function evaluateReplaceTextFlow(
  text: string,
  slots: ReplaceTextFlowSlot[],
  fontPx: number,
  safeScale: number,
  measure: TextMeasure,
): ReplaceTextFlowLayout {
  const tokens = tokenizeReplaceText(text)
  const lineHeight = Math.max(fontPx * 1.18, measuredBounds(measure, text, fontPx * safeScale).height / safeScale)
  // The renderer draws at the final size. Hinting and fallback fonts need not
  // scale linearly, so measure that size before converting to virtual units.
  const scaledMeasure = (value: string, size: number) => measuredBounds(measure, value, size * safeScale).width / safeScale
  const layouts = slots.map(slot => {
    const virtualWidth = Math.max(1, slot.width / safeScale)
    const virtualHeight = Math.max(1, slot.height / safeScale)
    const lineCount = Math.max(1, Math.floor(virtualHeight / lineHeight))
    const lines: string[] = []
    for (let index = 0; index < lineCount && tokens.length > 0; index += 1) {
      lines.push(takeReplaceFlowLine(tokens, virtualWidth, fontPx, scaledMeasure))
    }
    return {
      lines,
      contentWidth: Math.max(0, ...lines.map(line => scaledMeasure(line, fontPx))),
      contentHeight: lines.length * lineHeight,
    }
  })
  return {
    fontPx,
    lineHeight,
    safeScale,
    slots: layouts,
    complete: tokens.length === 0 && layouts.every((layout, index) =>
      layout.contentWidth * safeScale <= slots[index].width &&
      layout.contentHeight * safeScale <= slots[index].height,
    ),
  }
}

/**
 * Flow one complete translation through independent source slots. Translation
 * grouping therefore provides context without replacing several source lines
 * with one tall, vertically-centred render rectangle.
 */
export function layoutReplaceTextFlow(
  text: string,
  slots: ReplaceTextFlowSlot[],
  sourceFontPx: number,
  measure: TextMeasure,
  preferredMinPx = 7,
): ReplaceTextFlowLayout {
  if (slots.length === 0) {
    return { fontPx: preferredMinPx, lineHeight: preferredMinPx * 1.18, safeScale: 1, slots: [], complete: text.length === 0 }
  }
  const tallest = Math.max(...slots.map(slot => slot.height))
  const maxFont = Math.max(preferredMinPx, Math.min(sourceFontPx || 16, tallest * 0.82, 48))
  let low = preferredMinPx
  let high = maxFont
  let best: ReplaceTextFlowLayout | null = null
  for (let index = 0; index < 10; index += 1) {
    const fontPx = (low + high) / 2
    const candidate = evaluateReplaceTextFlow(text, slots, fontPx, 1, measure)
    if (candidate.complete) {
      best = candidate
      low = fontPx
    } else {
      high = fontPx
    }
  }
  if (best) return best

  let fittingScale = 1
  let scaled = evaluateReplaceTextFlow(text, slots, preferredMinPx, fittingScale, measure)
  while (!scaled.complete && fittingScale > 0.0001) {
    fittingScale /= 2
    scaled = evaluateReplaceTextFlow(text, slots, preferredMinPx, fittingScale, measure)
  }
  let scaleLow = fittingScale
  let scaleHigh = Math.min(1, fittingScale * 2)
  let scaledBest = scaled
  for (let index = 0; index < 12; index += 1) {
    const scale = (scaleLow + scaleHigh) / 2
    const candidate = evaluateReplaceTextFlow(text, slots, preferredMinPx, scale, measure)
    if (candidate.complete) {
      scaledBest = candidate
      scaleLow = scale
    } else {
      scaleHigh = scale
    }
  }
  return scaledBest
}

/** 框选命中的译文：任一 slot 与选框相交的 group 全文入选，保持 groups 的阅读顺序。 */
export function selectedGroupsText(
  groups: LensReplaceGroup[],
  slots: LensReplaceRenderSlot[],
  rect: { x: number; y: number; width: number; height: number },
  useSource: boolean,
): string {
  const hitGroupIds = new Set<string>()
  for (const slot of slots) {
    const { bounds } = slot
    const intersects =
      bounds.x < rect.x + rect.width &&
      bounds.x + bounds.width > rect.x &&
      bounds.y < rect.y + rect.height &&
      bounds.y + bounds.height > rect.y
    if (intersects) hitGroupIds.add(slot.groupId)
  }
  return groups
    .filter(group => hitGroupIds.has(group.id))
    .map(group => (useSource ? group.sourceText : group.translated.trim() || group.sourceText))
    .filter(Boolean)
    .join('\n')
}
