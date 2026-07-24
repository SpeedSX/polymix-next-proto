import { z } from 'zod'

import i18n from '../../lib/i18n'
import { MONEY_DECIMAL_PATTERN, toMinorUnits } from '../../lib/money'
import type { StatusTone } from '../../components/StatusBadge'

export const QUOTE_STATUS = {
  Draft: 0,
  Sent: 1,
  Accepted: 2,
  Declined: 3,
  Expired: 4,
} as const

export type QuoteStatusId = (typeof QUOTE_STATUS)[keyof typeof QUOTE_STATUS]

export const CURRENCY_OPTIONS = ['EUR', 'USD', 'GBP', 'UAH'] as const

// --- Engine shapes (quote-engine crate; carried verbatim over the wire) ------

/** Front/back ink counts as the engine's `"F/B"` string form. */
export type ColorsSpec = string

export interface SpecComponent {
  role: string
  pages: number
  colors: ColorsSpec
  material: string
  machine?: string | null
}

export interface OperationInstance {
  operation: string
  params?: Record<string, unknown>
}

export interface JobSpec {
  format: string
  quantity: number
  components: SpecComponent[]
  operations: OperationInstance[]
  technology_allow?: string[] | null
}

export type Selection = Record<string, unknown>

export interface ComponentCost {
  role: string
  machine_id: string | null
  sheets: number
  cost_micro: number
}

export interface OperationCost {
  operation: string
  cost_micro: number
}

export interface Breakdown {
  components: ComponentCost[]
  operations: OperationCost[]
  cost_micro: number
  total_minor: number
  unit_minor: number
}

// --- Adjustment --------------------------------------------------------------

export type AdjustmentKind = 'margin_override' | 'discount' | 'price_override'

export interface Adjustment {
  kind: AdjustmentKind
  /** margin_override */
  multiplier_bp?: number
  /** discount */
  percent_bp?: number
  /** price_override, minor units */
  total_minor?: number
  reason?: string | null
}

export interface EnginePricing {
  breakdown: Breakdown
  engine_total_minor: number
  adjustment?: Adjustment | null
  final_total_minor: number
}

// --- Quote lines (stored, priced) --------------------------------------------

export interface TemplateLine {
  kind: 'template'
  line_id: string
  template: string
  selection: Selection
  qty: number
  pricing: EnginePricing
}

export interface SpecLine {
  kind: 'spec'
  line_id: string
  job_spec: JobSpec
  description: string
  qty: number
  pricing: EnginePricing
}

export interface ManualLine {
  kind: 'manual'
  line_id: string
  description: string
  qty: number
  unit_minor: number
}

export type QuoteLine = TemplateLine | SpecLine | ManualLine

export function lineTotalMinor(line: QuoteLine): number {
  switch (line.kind) {
    case 'manual':
      return line.qty * line.unit_minor
    default:
      return line.pricing.final_total_minor
  }
}

export function lineDescription(line: QuoteLine): string {
  switch (line.kind) {
    case 'template':
      return line.template
    default:
      return line.description
  }
}

// --- Quote -------------------------------------------------------------------

export interface Prospect {
  name: string
  email?: string | null
  phone?: string | null
}

export interface Quote {
  id: string
  number: string
  customer_id?: string | null
  customer_name?: string | null
  prospect?: Prospect | null
  currency: string
  status: QuoteStatusId
  valid_until?: string | null
  lines: QuoteLine[]
  pricelist_version?: number | null
  notes?: string | null
  created_by: string
  revises?: string | null
  order_id?: string | null
  total_minor: number
  created_at: string
  updated_at: string
}

// --- Create/update payloads --------------------------------------------------

export type NewQuoteLine =
  | { kind: 'template'; line_id?: string; template: string; selection: Selection; qty: number; adjustment?: Adjustment | null }
  | { kind: 'spec'; line_id?: string; job_spec: JobSpec; description: string; qty: number; adjustment?: Adjustment | null }
  | { kind: 'manual'; line_id?: string; description: string; qty: number; unit_minor: number }

export interface NewQuote {
  customer_id?: string | null
  prospect?: Prospect | null
  currency?: string
  valid_until?: string | null
  notes?: string | null
  lines: NewQuoteLine[]
}

export interface QuoteListParams {
  page: number
  limit: number
  sort: string
  customer_id?: string
  status?: QuoteStatusId
  q?: string
  [key: string]: string | number | undefined
}

export interface QuoteListResponse {
  items: Quote[]
  total: number
  page: number
  limit: number
}

export interface RepriceResponse {
  quote: Quote
  changed_line_ids: string[]
}

// --- Estimate (stateless composer pricing) -----------------------------------

export interface EstimateResult {
  qty: number
  total_minor: number
  unit_minor: number
  breakdown: Breakdown
}

export interface EstimateResponse {
  currency: string
  policy_name?: string
  pricelist_version: number
  results: EstimateResult[]
}

export interface EstimateBody {
  job_spec: JobSpec
  quantities: number[]
  margin_override_bp?: number
}

// --- Status dictionary (static — quotes have no backend dictionary) ----------

export interface QuoteStatusMeta {
  id: QuoteStatusId
  key: string
  tone: StatusTone
  allowedTargets: QuoteStatusId[]
}

export const QUOTE_STATUS_META: Record<QuoteStatusId, QuoteStatusMeta> = {
  [QUOTE_STATUS.Draft]: { id: QUOTE_STATUS.Draft, key: 'draft', tone: 'neutral', allowedTargets: [QUOTE_STATUS.Sent] },
  [QUOTE_STATUS.Sent]: {
    id: QUOTE_STATUS.Sent,
    key: 'sent',
    tone: 'outline',
    allowedTargets: [QUOTE_STATUS.Accepted, QUOTE_STATUS.Declined, QUOTE_STATUS.Expired],
  },
  [QUOTE_STATUS.Accepted]: { id: QUOTE_STATUS.Accepted, key: 'accepted', tone: 'accent', allowedTargets: [QUOTE_STATUS.Expired] },
  [QUOTE_STATUS.Declined]: { id: QUOTE_STATUS.Declined, key: 'declined', tone: 'neutral', allowedTargets: [] },
  [QUOTE_STATUS.Expired]: { id: QUOTE_STATUS.Expired, key: 'expired', tone: 'neutral', allowedTargets: [] },
}

export function canEditQuote(status: QuoteStatusId): boolean {
  return status === QUOTE_STATUS.Draft
}

// --- Quote → NewQuote (round-trip for the draft editor) ----------------------

export function lineToNewLine(line: QuoteLine): NewQuoteLine {
  switch (line.kind) {
    case 'template':
      return {
        kind: 'template',
        line_id: line.line_id,
        template: line.template,
        selection: line.selection,
        qty: line.qty,
        adjustment: line.pricing.adjustment ?? null,
      }
    case 'spec':
      return {
        kind: 'spec',
        line_id: line.line_id,
        job_spec: line.job_spec,
        description: line.description,
        qty: line.qty,
        adjustment: line.pricing.adjustment ?? null,
      }
    case 'manual':
      return {
        kind: 'manual',
        line_id: line.line_id,
        description: line.description,
        qty: line.qty,
        unit_minor: line.unit_minor,
      }
  }
}

export function quoteToNewQuote(quote: Quote): NewQuote {
  return {
    customer_id: quote.customer_id ?? null,
    prospect: quote.prospect ?? null,
    currency: quote.currency,
    valid_until: quote.valid_until ?? null,
    notes: quote.notes,
    lines: quote.lines.map(lineToNewLine),
  }
}

// --- Forms -------------------------------------------------------------------

export const PARTY_MODES = ['customer', 'prospect'] as const
export type PartyMode = (typeof PARTY_MODES)[number]

export const quoteHeaderSchema = z
  .object({
    partyMode: z.enum(PARTY_MODES),
    customerId: z.string(),
    prospectName: z.string(),
    prospectEmail: z.string(),
    prospectPhone: z.string(),
    currency: z.string().length(3),
    validUntil: z.string(),
    notes: z.string(),
  })
  .superRefine((values, ctx) => {
    const partyRequired = i18n.t('quotes:errors.party_required')
    if (values.partyMode === 'customer' && values.customerId.trim() === '') {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['customerId'], message: partyRequired })
    }
    if (values.partyMode === 'prospect' && values.prospectName.trim() === '') {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['prospectName'], message: partyRequired })
    }
  })

export type QuoteHeaderValues = z.infer<typeof quoteHeaderSchema>

export function emptyQuoteHeaderValues(defaultCurrency: string): QuoteHeaderValues {
  return {
    partyMode: 'customer',
    customerId: '',
    prospectName: '',
    prospectEmail: '',
    prospectPhone: '',
    currency: defaultCurrency,
    validUntil: '',
    notes: '',
  }
}

export function quoteHeaderFrom(quote: Quote): QuoteHeaderValues {
  return {
    partyMode: quote.prospect ? 'prospect' : 'customer',
    customerId: quote.customer_id ?? '',
    prospectName: quote.prospect?.name ?? '',
    prospectEmail: quote.prospect?.email ?? '',
    prospectPhone: quote.prospect?.phone ?? '',
    currency: quote.currency,
    validUntil: quote.valid_until ?? '',
    notes: quote.notes ?? '',
  }
}

/** Party + document fields from the header form, merged into a NewQuote body. */
export function headerToNewQuoteFields(values: QuoteHeaderValues): Omit<NewQuote, 'lines'> {
  const prospect =
    values.partyMode === 'prospect'
      ? {
          name: values.prospectName.trim(),
          email: values.prospectEmail.trim() || null,
          phone: values.prospectPhone.trim() || null,
        }
      : null
  return {
    customer_id: values.partyMode === 'customer' ? values.customerId : null,
    prospect,
    currency: values.currency.toUpperCase(),
    valid_until: values.validUntil.trim() || null,
    notes: values.notes.trim() || null,
  }
}

export const manualLineSchema = z.object({
  description: z.string().trim().min(1),
  quantity: z.coerce.number().int().min(1),
  unitPrice: z.string().regex(MONEY_DECIMAL_PATTERN),
})

export type ManualLineValues = z.infer<typeof manualLineSchema>

export const emptyManualLineValues: ManualLineValues = { description: '', quantity: 1, unitPrice: '' }

export function manualLineToNew(values: ManualLineValues, currency: string, locale: string, lineId?: string): NewQuoteLine {
  return {
    kind: 'manual',
    line_id: lineId,
    description: values.description.trim(),
    qty: values.quantity,
    unit_minor: toMinorUnits(values.unitPrice, currency, locale),
  }
}
