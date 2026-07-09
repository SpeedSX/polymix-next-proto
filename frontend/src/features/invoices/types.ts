import { z } from 'zod'

import { fromMinorUnits, MONEY_DECIMAL_PATTERN, toMinorUnits } from '../../lib/money'

export const INVOICE_STATUSES = ['draft', 'issued', 'paid', 'void'] as const
export type InvoiceStatus = (typeof INVOICE_STATUSES)[number]

// Mirrors domain::invoice::validate_transition — kept in sync manually since
// the frontend has no access to the Rust domain crate.
export const INVOICE_TRANSITIONS: Record<InvoiceStatus, InvoiceStatus[]> = {
  draft: ['issued', 'void'],
  issued: ['paid', 'void'],
  paid: [],
  void: [],
}

export interface Money {
  amount_minor: number
  currency: string
}

export interface LineItem {
  description: string
  quantity: number
  unit_price: Money
}

export interface Invoice {
  id: string
  number: string
  order_id: string
  customer_id: string
  status: InvoiceStatus
  currency: string
  exchange_rate: string | null
  line_items: LineItem[]
  net_total: Money
  tax_rate_bp: number
  tax_total: Money
  gross_total: Money
  issue_date: string | null
  due_date: string | null
  created_at: string
  updated_at: string
}

export interface InvoiceListParams {
  page: number
  limit: number
  sort: string
  customer_id?: string
  status?: InvoiceStatus
  q?: string
  [key: string]: string | number | undefined
}

export interface InvoiceListResponse {
  items: Invoice[]
  total: number
  page: number
  limit: number
}

export interface UpdateInvoice {
  line_items: LineItem[]
}

export const lineItemFormSchema = z.object({
  description: z.string().min(1),
  quantity: z.coerce.number().int().min(1),
  unitPrice: z.string().regex(MONEY_DECIMAL_PATTERN),
})

// PUT /api/invoices/{id} only edits line items — order, customer, currency,
// and tax rate are set at creation/issuance and not part of this form. See
// docs/adr/0005-invoice-put-drafts-only.md.
export const invoiceFormSchema = z.object({
  lineItems: z.array(lineItemFormSchema).min(1),
})

export type InvoiceFormValues = z.infer<typeof invoiceFormSchema>

export function fromInvoice(invoice: Invoice, locale = 'en'): InvoiceFormValues {
  return {
    lineItems: invoice.line_items.map((item) => ({
      description: item.description,
      quantity: item.quantity,
      unitPrice: fromMinorUnits(item.unit_price.amount_minor, item.unit_price.currency, locale),
    })),
  }
}

export function toUpdateInvoice(values: InvoiceFormValues, currency: string, locale = 'en'): UpdateInvoice {
  return {
    line_items: values.lineItems.map((item) => ({
      description: item.description,
      quantity: item.quantity,
      unit_price: { amount_minor: toMinorUnits(item.unitPrice, currency, locale), currency },
    })),
  }
}

const LINE_ITEM_FIELD_PATTERN = /^line_items\[(\d+)\]\.(.+)$/

/** Mirrors orders/types.ts's mapApiErrorField — see that doc comment for the mapping rationale. */
export function mapApiErrorField(field: string): string | null {
  const lineItemMatch = field.match(LINE_ITEM_FIELD_PATTERN)
  if (lineItemMatch) {
    const [, index, rest] = lineItemMatch
    if (rest === 'description' || rest === 'quantity') {
      return `lineItems.${index}.${rest}`
    }
    if (rest.startsWith('unit_price')) {
      return `lineItems.${index}.unitPrice`
    }
    return null
  }
  return null
}
