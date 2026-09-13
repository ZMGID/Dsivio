import { describe, expect, it } from 'vitest'
import {
  layoutReplaceTextFlow,
  replaceTextVerticalOffset,
  tokenizeReplaceText,
} from './replaceTextLayout'

const measure = (text: string, fontPx: number) => text.length * fontPx * 0.55

describe('replace text tokenization', () => {
  it('keeps CJK characters independently breakable and Latin identifiers intact', () => {
    expect(tokenizeReplaceText('网络 web_search 工具')).toEqual(['网', '络', ' ', 'web_search', ' ', '工', '具'])
  })

  it('emits explicit paragraph breaks as standalone newline tokens', () => {
    expect(tokenizeReplaceText('first\nsecond')).toEqual(['first', '\n', 'second'])
  })
})

describe('replace text vertical placement', () => {
  it('top-aligns paragraphs so merged OCR does not shift the first line', () => {
    expect(replaceTextVerticalOffset('paragraph', 500, 320)).toBe(0)
  })

  it('keeps short labels and cells vertically centered', () => {
    expect(replaceTextVerticalOffset('line', 60, 40)).toBe(10)
    expect(replaceTextVerticalOffset('cell', 60, 40)).toBe(10)
  })
})

describe('replace text multi-slot flow', () => {
  it('fits unusually tall glyph ink instead of relying only on an estimated line height', () => {
    const layout = layoutReplaceTextFlow('a\u0301\u0302', [{ width: 60, height: 16 }], 16,
      (text, size) => ({ width: text.length * size * 0.5, height: size * 2.5 }))
    expect(layout.complete).toBe(true)
    expect(layout.fontPx * layout.safeScale * 2.5).toBeLessThanOrEqual(16)
  })

  it('measures scaled glyphs at the font size that will actually be drawn', () => {
    // Font hinting need not scale linearly. Keep a fixed pixel contribution.
    const hintedMeasure = (text: string, size: number) => text.length * (size * 0.55 + 1)
    const layout = layoutReplaceTextFlow('完整译文'.repeat(20), [{ width: 40, height: 16 }], 16, hintedMeasure)
    expect(layout.complete).toBe(true)
    expect(layout.safeScale).toBeLessThan(1)
    for (const line of layout.slots[0].lines) {
      expect(hintedMeasure(line, layout.fontPx * layout.safeScale)).toBeLessThanOrEqual(40)
    }
  })

  it('fits the shortest occupied slot instead of sizing every line from the tallest', () => {
    const layout = layoutReplaceTextFlow('译\n文', [{ width: 100, height: 8 }, { width: 100, height: 40 }], 24, measure)
    expect(layout.complete).toBe(true)
    layout.slots.forEach((slot, index) => {
      expect(slot.contentHeight * layout.safeScale).toBeLessThanOrEqual([8, 40][index])
    })
  })

  it('fits even a single glyph within a narrow slot before reporting complete', () => {
    const layout = layoutReplaceTextFlow('译', [{ width: 2, height: 20 }], 16, measure)
    expect(layout.complete).toBe(true)
    expect(layout.slots[0].contentWidth * layout.safeScale).toBeLessThanOrEqual(2)
  })

  it('keeps a translation group while preserving each source-line slot', () => {
    const text = '第一行译文和第二行译文必须按原来的两个位置流动'
    const layout = layoutReplaceTextFlow(
      text,
      [{ width: 110, height: 22 }, { width: 110, height: 22 }],
      14,
      measure,
    )
    expect(layout.complete).toBe(true)
    expect(layout.slots).toHaveLength(2)
    expect(layout.slots.flatMap(slot => slot.lines).join('')).toBe(text)
  })

  it('uses a shared safe scale rather than dropping the tail from the last slot', () => {
    const text = '完整译文'.repeat(80)
    const layout = layoutReplaceTextFlow(
      text,
      [{ width: 60, height: 18 }, { width: 60, height: 18 }],
      16,
      measure,
    )
    expect(layout.complete).toBe(true)
    expect(layout.safeScale).toBeLessThan(1)
    expect(layout.slots.flatMap(slot => slot.lines).join('')).toBe(text)
  })

  it('flows a single over-wide unbreakable token character by character', () => {
    // A lone long token (no spaces to break on) must still be laid out fully
    // by splitting characters across lines — not dropped or truncated.
    const url = 'httpsexamplecomverylongpathwithnobreaks'
    const layout = layoutReplaceTextFlow(url, [{ width: 40, height: 90 }], 16, measure)
    expect(layout.complete).toBe(true)
    expect(layout.slots[0].lines.join('')).toBe(url)
  })
})
