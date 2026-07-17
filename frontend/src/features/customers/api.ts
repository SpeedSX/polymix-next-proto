import type { useApi } from '../../lib/api'
import type { CustomerActivity } from '../orders/types'
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
  activity: (id: string) => ['customers', id, 'activity'] as const,
  statusDictionary: () => ['dictionaries', 'customer-statuses'] as const,
}

export function fetchCustomers(api: Api, params: CustomerListParams) {
  return api<CustomerListResponse>('/api/customers', { params })
}

export function fetchCustomer(api: Api, id: string) {
  return api<Customer>(`/api/customers/${id}`)
}

export function fetchCustomerActivity(api: Api, id: string) {
  return api<CustomerActivity>(`/api/customers/${id}/activity`)
}

export function createCustomer(api: Api, data: NewCustomer) {
  return api<Customer>('/api/customers', { method: 'POST', body: data })
}

export function updateCustomer(api: Api, id: string, data: NewCustomer, expectedVersion: number) {
  return api<Customer>(`/api/customers/${id}`, {
    method: 'PUT',
    body: data,
    // Optimistic concurrency: the server rejects (409 customer_modified) if
    // the record moved on since this version was loaded.
    headers: { 'if-match': String(expectedVersion) },
  })
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
