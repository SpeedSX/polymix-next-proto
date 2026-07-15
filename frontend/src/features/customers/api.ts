import type { useApi } from '../../lib/api'
import type {
  Customer,
  CustomerListParams,
  CustomerListResponse,
  CustomerStatusDictionaryResponse,
  CustomerStatusId,
  NewCustomer,
} from './types'

type Api = ReturnType<typeof useApi>

export const customersKeys = {
  all: ['customers'] as const,
  list: (params: CustomerListParams) => ['customers', params] as const,
  detail: (id: string) => ['customers', id] as const,
  statusDictionary: () => ['dictionaries', 'customer-statuses'] as const,
}

export function fetchCustomers(api: Api, params: CustomerListParams) {
  return api<CustomerListResponse>('/api/customers', { params })
}

export function fetchCustomer(api: Api, id: string) {
  return api<Customer>(`/api/customers/${id}`)
}

export function createCustomer(api: Api, data: NewCustomer) {
  return api<Customer>('/api/customers', { method: 'POST', body: data })
}

export function updateCustomer(api: Api, id: string, data: NewCustomer) {
  return api<Customer>(`/api/customers/${id}`, { method: 'PUT', body: data })
}

export function deleteCustomer(api: Api, id: string) {
  return api<void>(`/api/customers/${id}`, { method: 'DELETE' })
}

export function setCustomerStatus(api: Api, id: string, status: CustomerStatusId) {
  return api<Customer>(`/api/customers/${id}/status`, { method: 'POST', body: { status } })
}

export function fetchCustomerStatusDictionary(api: Api) {
  return api<CustomerStatusDictionaryResponse>('/api/dictionaries/customer-statuses')
}
