import { describe, expect, it } from 'vitest'

import { formatMoney, fromMinorUnits, toMinorUnits } from '.'

describe('money', () => {
  it('formats and converts amounts for a valid currency', () => {
    expect(toMinorUnits('12.34', 'USD')).toBe(1234)
    expect(fromMinorUnits(1234, 'USD')).toBe('12.34')
    expect(formatMoney({ amount_minor: 1234, currency: 'USD' })).toContain('12.34')
  })

  it('does not throw for an invalid or partially-typed currency code', () => {
    expect(() => toMinorUnits('12.34', 'U')).not.toThrow()
    expect(() => toMinorUnits('12.34', '')).not.toThrow()
    expect(() => fromMinorUnits(1234, 'EU')).not.toThrow()
    expect(() => formatMoney({ amount_minor: 1234, currency: 'U' })).not.toThrow()
  })

  it('treats a comma decimal separator the same as a dot, without dropping cents', () => {
    expect(toMinorUnits('12,34', 'USD')).toBe(1234)
    expect(toMinorUnits('12.34', 'USD')).toBe(toMinorUnits('12,34', 'USD'))
  })
})
