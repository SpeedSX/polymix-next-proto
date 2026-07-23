import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { renderHighlight } from './utils'

describe('renderHighlight', () => {
  it('renders <b> fragments as <strong>, never as raw HTML', () => {
    const { container } = render(<div>{renderHighlight('Acme <b>Corp</b> Ltd')}</div>)
    const strong = container.querySelector('strong')
    expect(strong).not.toBeNull()
    expect(strong?.textContent).toBe('Corp')
    // The literal tag must not survive as text or unparsed markup.
    expect(container.querySelector('b')).toBeNull()
    expect(container.textContent).toBe('Acme Corp Ltd')
  })

  it('wraps plain (non-highlighted) text in spans', () => {
    const { container } = render(<div>{renderHighlight('plain text')}</div>)
    expect(container.querySelector('strong')).toBeNull()
    expect(container.querySelector('span')?.textContent).toBe('plain text')
  })

  it('handles multiple highlighted fragments', () => {
    const { container } = render(<div>{renderHighlight('<b>a</b> and <b>b</b>')}</div>)
    const strongs = container.querySelectorAll('strong')
    expect(strongs).toHaveLength(2)
    expect(Array.from(strongs, (el) => el.textContent)).toEqual(['a', 'b'])
    expect(container.textContent).toBe('a and b')
  })

  it('drops empty split fragments (e.g. a leading highlight)', () => {
    // Splitting "<b>x</b>y" yields a leading empty string that must be filtered
    // out, otherwise React would render an empty span.
    const { container } = render(<div>{renderHighlight('<b>x</b>y')}</div>)
    expect(container.querySelector('strong')?.textContent).toBe('x')
    const spans = Array.from(container.querySelectorAll('span'), (el) => el.textContent)
    expect(spans).toEqual(['y'])
  })

  it('renders an empty string to no visible text', () => {
    const { container } = render(<div>{renderHighlight('')}</div>)
    expect(container.textContent).toBe('')
  })
})
