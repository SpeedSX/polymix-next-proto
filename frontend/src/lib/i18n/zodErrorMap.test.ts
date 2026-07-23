import i18n from 'i18next'
import { beforeAll, describe, expect, it } from 'vitest'
import { z } from 'zod'

import { zodErrorMap } from './zodErrorMap'

// Map every validation code to a distinct sentinel so the resolved code is
// unambiguous in assertions, independent of the real translation catalogue.
const validation: Record<string, string> = {
  required: 'REQUIRED',
  out_of_range: 'OUT_OF_RANGE',
  min_line_items: 'MIN_LINE_ITEMS',
  positive_quantity: 'POSITIVE_QUANTITY',
  invalid_country_code: 'INVALID_COUNTRY_CODE',
  invalid_currency_code: 'INVALID_CURRENCY_CODE',
  invalid_email: 'INVALID_EMAIL',
  invalid_amount: 'INVALID_AMOUNT',
}

beforeAll(async () => {
  if (!i18n.isInitialized) {
    await i18n.init({ lng: 'en', resources: { en: { common: { validation } } } })
  }
})

/** Runs a parse that is guaranteed to fail and returns the mapped message. */
function messageFor(schema: z.ZodTypeAny, value: unknown): string {
  const result = schema.safeParse(value, { errorMap: zodErrorMap })
  expect(result.success).toBe(false)
  if (result.success) throw new Error('unreachable')
  return result.error.issues[0].message
}

describe('zodErrorMap', () => {
  it('maps a missing value to `required`', () => {
    expect(messageFor(z.string(), undefined)).toBe('REQUIRED')
  })

  it('maps a wrong numeric type to `out_of_range`', () => {
    expect(messageFor(z.number(), 'not a number')).toBe('OUT_OF_RANGE')
  })

  it('maps an empty array to `min_line_items`', () => {
    expect(messageFor(z.array(z.string()).min(1), [])).toBe('MIN_LINE_ITEMS')
  })

  it('maps an empty required string to `required`', () => {
    expect(messageFor(z.string().min(1), '')).toBe('REQUIRED')
  })

  it('maps an exact length-2 string to `invalid_country_code`', () => {
    expect(messageFor(z.string().length(2), 'x')).toBe('INVALID_COUNTRY_CODE')
  })

  it('maps an exact length-3 string to `invalid_currency_code`', () => {
    expect(messageFor(z.string().length(3), 'xx')).toBe('INVALID_CURRENCY_CODE')
    // Too-big side of an exact-length constraint resolves the same way.
    expect(messageFor(z.string().length(3), 'xxxx')).toBe('INVALID_CURRENCY_CODE')
  })

  it('maps an exact length other than 2/3 to `out_of_range`', () => {
    expect(messageFor(z.string().length(4), 'x')).toBe('OUT_OF_RANGE')
  })

  it('maps a below-one number to `positive_quantity`', () => {
    expect(messageFor(z.number().min(1), 0)).toBe('POSITIVE_QUANTITY')
  })

  it('maps other numeric bounds to `out_of_range`', () => {
    expect(messageFor(z.number().min(5), 3)).toBe('OUT_OF_RANGE')
    expect(messageFor(z.number().max(10), 20)).toBe('OUT_OF_RANGE')
  })

  it('maps a bad email to `invalid_email`', () => {
    expect(messageFor(z.string().email(), 'nope')).toBe('INVALID_EMAIL')
  })

  it('maps other invalid strings to `invalid_amount`', () => {
    expect(messageFor(z.string().regex(/^\d+$/), 'abc')).toBe('INVALID_AMOUNT')
  })

  it('drills into a union to find the meaningful branch (empty-string escape hatch)', () => {
    const field = z.union([z.literal(''), z.string().email()])
    expect(messageFor(field, 'not-an-email')).toBe('INVALID_EMAIL')
  })

  it('falls back to Zod default for codes it does not translate', () => {
    // string-expected/number-received is deliberately unmapped -> default text.
    const msg = messageFor(z.string(), 42)
    expect(Object.values(validation)).not.toContain(msg)
    expect(msg.length).toBeGreaterThan(0)
  })

  it('falls back to Zod default for an unmapped array upper bound', () => {
    // `too_big` on an array hits neither the string nor number branch.
    const msg = messageFor(z.array(z.string()).max(1), ['a', 'b'])
    expect(Object.values(validation)).not.toContain(msg)
  })
})
