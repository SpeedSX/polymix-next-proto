import { z } from 'zod'

import { fromMinorUnits, MONEY_DECIMAL_PATTERN, toMinorUnits } from '../../lib/money'

export const ORDER_STATUSES = ['draft', 'confirmed', 'in_production', 'completed', 'cancelled'] as const
export type OrderStatus = (typeof ORDER_STATUSES)[number]

// Mirrors domain::order::validate_transition — kept in sync manually since
// the frontend has no access to the Rust domain crate.
export const ORDER_TRANSITIONS: Record<OrderStatus, OrderStatus[]> = {
  draft: ['confirmed', 'cancelled'],
  confirmed: ['in_production', 'cancelled'],
  in_production: ['completed'],
  completed: [],
  cancelled: [],
}

// Mirrors domain::order::can_invoice.
export const INVOICEABLE_STATUSES: OrderStatus[] = ['confirmed', 'in_production', 'completed']

export interface Money {
  amount_minor: number
  currency: string
}

export interface LineItem {
  description: string
  quantity: number
  unit_price: Money
}

export interface Order {
  id: string
  number: string
  customer_id: string
  status: OrderStatus
  currency: string
  line_items: LineItem[]
  total: Money
  notes: string | null
  created_at: string
  updated_at: string
}

export interface NewOrder {
  customer_id: string
  currency?: string
  line_items: LineItem[]
  notes: string | null
}

export interface OrderListParams {
  page: number
  limit: number
  sort: string
  customer_id?: string
  status?: OrderStatus
  q?: string
  [key: string]: string | number | undefined
}

export interface OrderListResponse {
  items: Order[]
  total: number
  page: number
  limit: number
}

export const lineItemFormSchema = z.object({
  description: z.string().min(1),
  quantity: z.coerce.number().int().min(1),
  unitPrice: z.string().regex(MONEY_DECIMAL_PATTERN),
})

export const orderFormSchema = z.object({
  customerId: z.string().min(1),
  currency: z.string().length(3),
  notes: z.string(),
  lineItems: z.array(lineItemFormSchema).min(1),
})

export type OrderFormValues = z.infer<typeof orderFormSchema>

export function emptyOrderFormValues(defaultCurrency: string): OrderFormValues {
  return {
    customerId: '',
    currency: defaultCurrency,
    notes: '',
    lineItems: [{ description: '', quantity: 1, unitPrice: '' }],
  }
}

export function toNewOrder(values: OrderFormValues): NewOrder {
  const currency = values.currency.toUpperCase()
  return {
    customer_id: values.customerId,
    currency,
    notes: values.notes === '' ? null : values.notes,
    line_items: values.lineItems.map((item) => ({
      description: item.description,
      quantity: item.quantity,
      unit_price: { amount_minor: toMinorUnits(item.unitPrice, currency), currency },
    })),
  }
}

export function fromOrder(order: Order): OrderFormValues {
  return {
    customerId: order.customer_id,
    currency: order.currency,
    notes: order.notes ?? '',
    lineItems: order.line_items.map((item) => ({
      description: item.description,
      quantity: item.quantity,
      unitPrice: fromMinorUnits(item.unit_price.amount_minor, item.unit_price.currency),
    })),
  }
}

const TOP_LEVEL_FIELD_MAP: Record<string, string> = {
  customer_id: 'customerId',
}

const LINE_ITEM_FIELD_PATTERN = /^line_items\[(\d+)\]\.(.+)$/

/**
 * Translates a backend validation error key to the Mantine form path it
 * corresponds to, e.g. `line_items[0].unit_price.currency` -> `lineItems.0.unitPrice`
 * (the form collects unit price as a single string field, not the nested
 * Money struct the backend validates). Returns null when the error targets
 * the line-item array as a whole rather than a single row/field — callers
 * should surface those as a form-level message instead of a field error.
 */
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
  if (field === 'line_items') {
    return null
  }
  return TOP_LEVEL_FIELD_MAP[field] ?? field
}
