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
