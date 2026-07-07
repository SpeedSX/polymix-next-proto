import type { ReactNode } from 'react'

const HIGHLIGHT_SPLIT = /(<b>.*?<\/b>)/g
const HIGHLIGHT_TAG = /^<b>(.*)<\/b>$/

/**
 * Renders a BM25-highlighted fragment as React nodes instead of raw HTML —
 * the API only ever emits `<b>` tags (PLAN.md's "safe subset"), but parsing
 * them ourselves means there's no `dangerouslySetInnerHTML` to get wrong if
 * that ever changes.
 */
export function renderHighlight(text: string): ReactNode {
  return text
    .split(HIGHLIGHT_SPLIT)
    .filter((part) => part !== '')
    .map((part, index) => {
      const match = HIGHLIGHT_TAG.exec(part)
      return match ? <strong key={index}>{match[1]}</strong> : <span key={index}>{part}</span>
    })
}
