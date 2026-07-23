import { describe, expect, it } from 'vitest'

import { formatDate, formatDateTime, formatTimestampDate } from '.'

describe('dates', () => {
  it('formats a timestamp with both date and time parts', () => {
    const out = formatDateTime('2026-01-15T09:30:00Z', 'en')
    expect(out).toContain('2026')
    // The time component means a `:` separator is always present.
    expect(out).toMatch(/\d{2}:\d{2}/)
  })

  it('formats only the date portion of a timestamp', () => {
    const out = formatTimestampDate('2026-01-15T09:30:00Z', 'en')
    expect(out).toContain('2026')
    // No time part, so no clock separator.
    expect(out).not.toMatch(/\d{2}:\d{2}/)
  })

  it('renders a date-only string as UTC midnight so the day never shifts', () => {
    // Parsed as `2026-01-15T00:00:00Z`; the day stays 15 regardless of the
    // runner timezone (this is the whole reason the helper appends `Z`).
    const utcMidday = formatTimestampDate('2026-01-15T12:00:00Z', 'en')
    expect(formatDate('2026-01-15', 'en')).toBe(utcMidday)
  })

  it('respects the requested locale', () => {
    // uk uses a different date order/formatting than en for the same instant.
    const en = formatDate('2026-01-15', 'en')
    const uk = formatDate('2026-01-15', 'uk')
    expect(en).not.toBe(uk)
    expect(uk).toContain('2026')
  })

  it('falls back to the raw value when the input is unparseable', () => {
    expect(formatDateTime('not-a-date', 'en')).toBe('not-a-date')
    expect(formatTimestampDate('nope', 'en')).toBe('nope')
    expect(formatDate('garbage', 'en')).toBe('garbage')
  })

  it('falls back to the raw value for an invalid locale tag', () => {
    // A malformed locale makes the Intl constructor throw, hitting the catch.
    expect(formatDateTime('2026-01-15T09:30:00Z', 'not a locale')).toBe(
      '2026-01-15T09:30:00Z',
    )
  })

  it('defaults to the en locale when none is passed', () => {
    expect(() => formatDateTime('2026-01-15T09:30:00Z')).not.toThrow()
    expect(formatDate('2026-01-15')).toContain('2026')
  })
})
