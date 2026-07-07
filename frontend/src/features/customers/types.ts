import { z } from 'zod'

export const addressFormSchema = z.object({
  street: z.string(),
  zip: z.string(),
  city: z.string(),
  country: z.union([z.literal(''), z.string().length(2)]),
})

export const customerFormSchema = z.object({
  name: z.string().min(1),
  contactName: z.string(),
  email: z.union([z.literal(''), z.string().email()]),
  phone: z.string(),
  notes: z.string(),
  address: addressFormSchema,
})

export type CustomerFormValues = z.infer<typeof customerFormSchema>

export const emptyCustomerFormValues: CustomerFormValues = {
  name: '',
  contactName: '',
  email: '',
  phone: '',
  notes: '',
  address: { street: '', zip: '', city: '', country: '' },
}

export interface Address {
  street: string | null
  zip: string | null
  city: string | null
  country: string | null
}

export interface Customer {
  id: string
  name: string
  contact_name: string | null
  email: string | null
  phone: string | null
  address: Address | null
  notes: string | null
  created_at: string
  updated_at: string
}

export interface NewCustomer {
  name: string
  contact_name: string | null
  email: string | null
  phone: string | null
  address: Address | null
  notes: string | null
}

export interface CustomerListParams {
  page: number
  limit: number
  sort: string
  q?: string
  [key: string]: string | number | undefined
}

export interface CustomerListResponse {
  items: Customer[]
  total: number
  page: number
  limit: number
}

function nullIfEmpty(value: string): string | null {
  return value === '' ? null : value
}

export function toNewCustomer(values: CustomerFormValues): NewCustomer {
  const address = values.address
  const hasAddress = address.street || address.zip || address.city || address.country
  return {
    name: values.name,
    contact_name: nullIfEmpty(values.contactName),
    email: nullIfEmpty(values.email),
    phone: nullIfEmpty(values.phone),
    notes: nullIfEmpty(values.notes),
    address: hasAddress
      ? {
          street: nullIfEmpty(address.street),
          zip: nullIfEmpty(address.zip),
          city: nullIfEmpty(address.city),
          country: nullIfEmpty(address.country),
        }
      : null,
  }
}

export function fromCustomer(customer: Customer): CustomerFormValues {
  return {
    name: customer.name,
    contactName: customer.contact_name ?? '',
    email: customer.email ?? '',
    phone: customer.phone ?? '',
    notes: customer.notes ?? '',
    address: {
      street: customer.address?.street ?? '',
      zip: customer.address?.zip ?? '',
      city: customer.address?.city ?? '',
      country: customer.address?.country ?? '',
    },
  }
}

const API_ERROR_FIELD_MAP: Record<string, string> = {
  contact_name: 'contactName',
}

export function mapApiErrorField(field: string): string {
  return API_ERROR_FIELD_MAP[field] ?? field
}
