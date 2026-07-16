import { describe, expect, it } from 'vitest'

import { convertedDisplay, formatMoney, fromMinorUnits, toMinorUnits } from '.'

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

  it('round-trips through the uk locale without losing cents', () => {
    const minor = toMinorUnits('12,34', 'UAH', 'uk')
    expect(minor).toBe(1234)
    expect(fromMinorUnits(minor, 'UAH', 'uk')).toBe('12.34')
  })

  it('converts a display-only amount using a base->quote rate snapshot', () => {
    // 1 EUR = 1.0842 USD, so 108.42 USD converts back to ~100.00 EUR.
    const display = convertedDisplay({ amount_minor: 10842, currency: 'USD' }, '1.0842', 'EUR')
    expect(display).toContain('100.00')
  })

  it('has no display when there is no rate snapshot', () => {
    expect(convertedDisplay({ amount_minor: 1000, currency: 'USD' }, null, 'EUR')).toBeNull()
  })
})
