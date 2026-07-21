import { describe, expect, it } from 'vitest'

import { orderStatusTone } from './statusTone'

describe('orderStatusTone', () => {
  it('maps each order status to its design-2a tone', () => {
    expect(orderStatusTone('draft')).toBe('neutral')
    expect(orderStatusTone('confirmed')).toBe('outline')
    expect(orderStatusTone('in_production')).toBe('accent')
    expect(orderStatusTone('completed')).toBe('accent')
    expect(orderStatusTone('cancelled')).toBe('neutral')
  })

  it('falls back to neutral for unknown or missing keys', () => {
    expect(orderStatusTone(undefined)).toBe('neutral')
    expect(orderStatusTone('shipped')).toBe('neutral')
  })
})
