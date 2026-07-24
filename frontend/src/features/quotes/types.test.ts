import { describe, expect, it } from 'vitest'

import {
  headerToNewQuoteFields,
  lineTotalMinor,
  manualLineToNew,
  quoteToNewQuote,
} from './types'
import type { EnginePricing, ManualLine, Quote, QuoteHeaderValues, SpecLine } from './types'

const pricing: EnginePricing = {
  breakdown: { components: [], operations: [], cost_micro: 100_000, total_minor: 900, unit_minor: 2 },
  engine_total_minor: 900,
  adjustment: { kind: 'discount', percent_bp: 1000, reason: 'loyal' },
  final_total_minor: 810,
}

const specLine: SpecLine = {
  kind: 'spec',
  line_id: 'l1',
  job_spec: { format: 'format:a5', quantity: 500, components: [], operations: [] },
  description: 'Booklet',
  qty: 500,
  pricing,
}

const manualLine: ManualLine = { kind: 'manual', line_id: 'l2', description: 'Delivery', qty: 2, unit_minor: 18_500 }

describe('lineTotalMinor', () => {
  it('uses the adjusted final total for engine lines', () => {
    expect(lineTotalMinor(specLine)).toBe(810)
  })

  it('multiplies qty by unit for manual lines', () => {
    expect(lineTotalMinor(manualLine)).toBe(37_000)
  })
})

describe('quoteToNewQuote', () => {
  const quote: Quote = {
    id: 'quote:1',
    number: 'Q-1',
    customer_id: 'customer:acme',
    customer_name: 'Acme',
    currency: 'EUR',
    status: 0,
    lines: [specLine, manualLine],
    created_by: 'user:1',
    total_minor: 37_810,
    created_at: '2026-07-01T00:00:00Z',
    updated_at: '2026-07-01T00:00:00Z',
  }

  it('round-trips lines, preserving ids and adjustments', () => {
    const next = quoteToNewQuote(quote)
    expect(next.customer_id).toBe('customer:acme')
    expect(next.lines).toHaveLength(2)
    const [spec, manual] = next.lines
    expect(spec).toMatchObject({ kind: 'spec', line_id: 'l1', adjustment: { kind: 'discount', percent_bp: 1000 } })
    expect(manual).toMatchObject({ kind: 'manual', line_id: 'l2', unit_minor: 18_500 })
  })
})

describe('headerToNewQuoteFields', () => {
  const base: QuoteHeaderValues = {
    partyMode: 'customer',
    customerId: 'customer:acme',
    prospectName: '',
    prospectEmail: '',
    prospectPhone: '',
    currency: 'eur',
    validUntil: '',
    notes: '  hello  ',
  }

  it('emits a customer id and uppercases currency in customer mode', () => {
    const fields = headerToNewQuoteFields(base)
    expect(fields).toMatchObject({ customer_id: 'customer:acme', prospect: null, currency: 'EUR', notes: 'hello' })
  })

  it('emits a prospect and no customer in prospect mode', () => {
    const fields = headerToNewQuoteFields({ ...base, partyMode: 'prospect', prospectName: 'Globex', prospectEmail: '' })
    expect(fields.customer_id).toBeNull()
    expect(fields.prospect).toMatchObject({ name: 'Globex', email: null })
  })
})

describe('manualLineToNew', () => {
  it('converts a decimal unit price to minor units', () => {
    const line = manualLineToNew({ description: 'Courier', quantity: 3, unitPrice: '18.50' }, 'EUR', 'en', 'keep')
    expect(line).toEqual({ kind: 'manual', line_id: 'keep', description: 'Courier', qty: 3, unit_minor: 1850 })
  })
})
