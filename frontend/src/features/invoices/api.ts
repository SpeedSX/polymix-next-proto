import type { useApi } from '../../lib/api'
import type { Invoice, InvoiceListParams, InvoiceListResponse, InvoiceStatus, UpdateInvoice } from './types'

type Api = ReturnType<typeof useApi>

export const invoicesKeys = {
  all: ['invoices'] as const,
  list: (params: InvoiceListParams) => ['invoices', params] as const,
  detail: (id: string) => ['invoices', id] as const,
}

export function fetchInvoices(api: Api, params: InvoiceListParams) {
  return api<InvoiceListResponse>('/api/invoices', { params })
}

export function fetchInvoice(api: Api, id: string) {
  return api<Invoice>(`/api/invoices/${id}`)
}

export function setInvoiceStatus(api: Api, id: string, status: InvoiceStatus) {
  return api<Invoice>(`/api/invoices/${id}/status`, { method: 'POST', body: { status } })
}

export function updateInvoice(api: Api, id: string, data: UpdateInvoice) {
  return api<Invoice>(`/api/invoices/${id}`, { method: 'PUT', body: data })
}
