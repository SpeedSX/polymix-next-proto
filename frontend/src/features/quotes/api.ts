import type { useApi } from '../../lib/api'
import type { Order } from '../orders/types'
import type {
  EstimateBody,
  EstimateResponse,
  NewQuote,
  Quote,
  QuoteListParams,
  QuoteListResponse,
  QuoteStatusId,
  RepriceResponse,
} from './types'

type Api = ReturnType<typeof useApi>

export const quotesKeys = {
  all: ['quotes'] as const,
  list: (params: QuoteListParams) => ['quotes', params] as const,
  detail: (id: string) => ['quotes', id] as const,
}

export function fetchQuotes(api: Api, params: QuoteListParams) {
  return api<QuoteListResponse>('/api/quotes', { params })
}

export function fetchQuote(api: Api, id: string) {
  return api<Quote>(`/api/quotes/${id}`)
}

export function createQuote(api: Api, data: NewQuote) {
  return api<Quote>('/api/quotes', { method: 'POST', body: data })
}

export function updateQuote(api: Api, id: string, data: NewQuote) {
  return api<Quote>(`/api/quotes/${id}`, { method: 'PUT', body: data })
}

export function deleteQuote(api: Api, id: string) {
  return api<void>(`/api/quotes/${id}`, { method: 'DELETE' })
}

export function setQuoteStatus(api: Api, id: string, status: QuoteStatusId) {
  return api<Quote>(`/api/quotes/${id}/status`, { method: 'POST', body: { status } })
}

export function repriceQuote(api: Api, id: string) {
  return api<RepriceResponse>(`/api/quotes/${id}/reprice`, { method: 'POST', body: {} })
}

export function cloneQuote(api: Api, id: string) {
  return api<Quote>(`/api/quotes/${id}/clone`, { method: 'POST', body: {} })
}

export function convertQuoteToOrder(api: Api, id: string) {
  return api<Order>(`/api/quotes/${id}/order`, { method: 'POST', body: {} })
}

export function estimate(api: Api, body: EstimateBody) {
  return api<EstimateResponse>('/api/estimate', { method: 'POST', body })
}
