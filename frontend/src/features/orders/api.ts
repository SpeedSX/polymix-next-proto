import type { useApi } from '../../lib/api'
import type { Invoice } from '../invoices/types'
import type {
  NewOrder,
  Order,
  OrderListParams,
  OrderListResponse,
  OrderStatusDictionaryResponse,
  OrderStatusId,
} from './types'

type Api = ReturnType<typeof useApi>

export const ordersKeys = {
  all: ['orders'] as const,
  list: (params: OrderListParams) => ['orders', params] as const,
  detail: (id: string) => ['orders', id] as const,
  statusDictionary: () => ['dictionaries', 'order-statuses'] as const,
}

export function fetchOrders(api: Api, params: OrderListParams) {
  return api<OrderListResponse>('/api/orders', { params })
}

export function fetchOrder(api: Api, id: string) {
  return api<Order>(`/api/orders/${id}`)
}

export function createOrder(api: Api, data: NewOrder) {
  return api<Order>('/api/orders', { method: 'POST', body: data })
}

export function updateOrder(api: Api, id: string, data: NewOrder) {
  return api<Order>(`/api/orders/${id}`, { method: 'PUT', body: data })
}

export function deleteOrder(api: Api, id: string) {
  return api<void>(`/api/orders/${id}`, { method: 'DELETE' })
}

export function setOrderStatus(api: Api, id: string, status: OrderStatusId) {
  return api<Order>(`/api/orders/${id}/status`, { method: 'POST', body: { status } })
}

export function fetchOrderStatusDictionary(api: Api) {
  return api<OrderStatusDictionaryResponse>('/api/dictionaries/order-statuses')
}

export function createInvoiceFromOrder(api: Api, orderId: string) {
  return api<Invoice>(`/api/orders/${orderId}/invoice`, { method: 'POST', body: {} })
}
