import { z } from 'zod'

import { fromMinorUnits, toMinorUnits } from '../../lib/money'

export const CUSTOMER_KIND = {
  LegalEntity: 0,
  Fop: 1,
  Individual: 2,
} as const

export type CustomerKindId = (typeof CUSTOMER_KIND)[keyof typeof CUSTOMER_KIND]

export const CUSTOMER_STATUS = {
  Lead: 0,
  Active: 1,
  Inactive: 2,
  Blocked: 3,
} as const

export type CustomerStatusId = (typeof CUSTOMER_STATUS)[keyof typeof CUSTOMER_STATUS]

export interface CustomerStatusDictionaryItem {
  id: CustomerStatusId
  key: string
  sort: number
  color: string
  can_order: boolean
  allowed_targets: CustomerStatusId[]
  labels: Record<string, string>
}

export interface CustomerStatusDictionaryResponse {
  items: CustomerStatusDictionaryItem[]
}

export interface Address {
  street: string | null
  zip: string | null
  city: string | null
  country: string | null
}

export interface Contact {
  name: string
  role: string | null
  email: string | null
  phone: string | null
  is_primary: boolean
}

export interface Money {
  amount_minor: number
  currency: string
}

export interface Customer {
  id: string
  number: string
  kind: CustomerKindId
  name: string
  legal_name: string | null
  edrpou: string | null
  tax_id: string | null
  vat_ipn: string | null
  status: CustomerStatusId
  tags: string[]
  industry: string | null
  source: string | null
  website: string | null
  contacts: Contact[]
  legal_address: Address | null
  delivery_address: Address | null
  payment_terms_days: number
  credit_limit: Money | null
  default_currency: string
  default_discount_bp: number
  iban: string | null
  bank_name: string | null
  notes: string | null
  created_at: string
  updated_at: string
}

export interface NewCustomer {
  kind: CustomerKindId
  name: string
  legal_name: string | null
  edrpou: string | null
  tax_id: string | null
  vat_ipn: string | null
  tags: string[]
  industry: string | null
  source: string | null
  website: string | null
  contacts: Contact[]
  legal_address: Address | null
  delivery_address: Address | null
  payment_terms_days: number
  credit_limit: Money | null
  default_currency?: string
  default_discount_bp: number
  iban: string | null
  bank_name: string | null
  notes: string | null
}

export interface CustomerListParams {
  page: number
  limit: number
  sort: string
  q?: string
  status?: CustomerStatusId
  tag?: string
  [key: string]: string | number | undefined
}

export interface CustomerListResponse {
  items: Customer[]
  total: number
  page: number
  limit: number
}

const addressFormSchema = z.object({
  street: z.string(),
  zip: z.string(),
  city: z.string(),
  country: z.union([z.literal(''), z.string().length(2)]),
})

export type AddressFormValues = z.infer<typeof addressFormSchema>

const emptyAddressFormValues: AddressFormValues = { street: '', zip: '', city: '', country: '' }

const contactFormSchema = z.object({
  name: z.string().min(1),
  role: z.string(),
  email: z.union([z.literal(''), z.string().email()]),
  phone: z.string(),
  isPrimary: z.boolean(),
})

export type ContactFormValues = z.infer<typeof contactFormSchema>

export const emptyContactFormValues: ContactFormValues = {
  name: '',
  role: '',
  email: '',
  phone: '',
  isPrimary: false,
}

export const customerFormSchema = z.object({
  kind: z.number(),
  name: z.string().min(1),
  legalName: z.string(),
  edrpou: z.string(),
  taxId: z.string(),
  vatIpn: z.string(),
  industry: z.string(),
  source: z.string(),
  website: z.string(),
  tags: z.array(z.string()),
  contacts: z.array(contactFormSchema),
  legalAddress: addressFormSchema,
  deliveryAddress: addressFormSchema,
  paymentTermsDays: z.coerce.number().int().min(0).max(365),
  hasCreditLimit: z.boolean(),
  creditLimitAmount: z.string(),
  creditLimitCurrency: z.string(),
  defaultCurrency: z.string().length(3),
  defaultDiscountPercent: z.string(),
  iban: z.string(),
  bankName: z.string(),
  notes: z.string(),
})

export type CustomerFormValues = z.infer<typeof customerFormSchema>

export function emptyCustomerFormValues(defaultCurrency: string): CustomerFormValues {
  return {
    kind: CUSTOMER_KIND.LegalEntity,
    name: '',
    legalName: '',
    edrpou: '',
    taxId: '',
    vatIpn: '',
    industry: '',
    source: '',
    website: '',
    tags: [],
    contacts: [],
    legalAddress: emptyAddressFormValues,
    deliveryAddress: emptyAddressFormValues,
    paymentTermsDays: 0,
    hasCreditLimit: false,
    creditLimitAmount: '',
    creditLimitCurrency: defaultCurrency,
    defaultCurrency,
    defaultDiscountPercent: '0',
    iban: '',
    bankName: '',
    notes: '',
  }
}

function nullIfEmpty(value: string): string | null {
  return value === '' ? null : value
}

function addressOrNull(address: AddressFormValues): Address | null {
  if (!address.street && !address.zip && !address.city && !address.country) {
    return null
  }
  return {
    street: nullIfEmpty(address.street),
    zip: nullIfEmpty(address.zip),
    city: nullIfEmpty(address.city),
    country: nullIfEmpty(address.country),
  }
}

function addressFormValues(address: Address | null): AddressFormValues {
  return {
    street: address?.street ?? '',
    zip: address?.zip ?? '',
    city: address?.city ?? '',
    country: address?.country ?? '',
  }
}

export function toNewCustomer(values: CustomerFormValues, locale = 'en'): NewCustomer {
  const currency = values.defaultCurrency.toUpperCase()
  return {
    kind: values.kind as CustomerKindId,
    name: values.name,
    legal_name: nullIfEmpty(values.legalName),
    edrpou: nullIfEmpty(values.edrpou),
    tax_id: nullIfEmpty(values.taxId),
    vat_ipn: nullIfEmpty(values.vatIpn),
    tags: values.tags,
    industry: nullIfEmpty(values.industry),
    source: nullIfEmpty(values.source),
    website: nullIfEmpty(values.website),
    contacts: values.contacts.map((contact) => ({
      name: contact.name,
      role: nullIfEmpty(contact.role),
      email: nullIfEmpty(contact.email),
      phone: nullIfEmpty(contact.phone),
      is_primary: contact.isPrimary,
    })),
    legal_address: addressOrNull(values.legalAddress),
    delivery_address: addressOrNull(values.deliveryAddress),
    payment_terms_days: values.paymentTermsDays,
    credit_limit: values.hasCreditLimit
      ? {
          amount_minor: toMinorUnits(values.creditLimitAmount, values.creditLimitCurrency.toUpperCase(), locale),
          currency: values.creditLimitCurrency.toUpperCase(),
        }
      : null,
    default_currency: currency,
    default_discount_bp: Math.round(Number.parseFloat(values.defaultDiscountPercent || '0') * 100),
    iban: nullIfEmpty(values.iban),
    bank_name: nullIfEmpty(values.bankName),
    notes: nullIfEmpty(values.notes),
  }
}

export function fromCustomer(customer: Customer, locale = 'en'): CustomerFormValues {
  return {
    kind: customer.kind,
    name: customer.name,
    legalName: customer.legal_name ?? '',
    edrpou: customer.edrpou ?? '',
    taxId: customer.tax_id ?? '',
    vatIpn: customer.vat_ipn ?? '',
    industry: customer.industry ?? '',
    source: customer.source ?? '',
    website: customer.website ?? '',
    tags: customer.tags,
    contacts: customer.contacts.map((contact) => ({
      name: contact.name,
      role: contact.role ?? '',
      email: contact.email ?? '',
      phone: contact.phone ?? '',
      isPrimary: contact.is_primary,
    })),
    legalAddress: addressFormValues(customer.legal_address),
    deliveryAddress: addressFormValues(customer.delivery_address),
    paymentTermsDays: customer.payment_terms_days,
    hasCreditLimit: customer.credit_limit !== null,
    creditLimitAmount: customer.credit_limit
      ? fromMinorUnits(customer.credit_limit.amount_minor, customer.credit_limit.currency, locale)
      : '',
    creditLimitCurrency: customer.credit_limit?.currency ?? customer.default_currency,
    defaultCurrency: customer.default_currency,
    defaultDiscountPercent: (customer.default_discount_bp / 100).toString(),
    iban: customer.iban ?? '',
    bankName: customer.bank_name ?? '',
    notes: customer.notes ?? '',
  }
}

const TOP_LEVEL_FIELD_MAP: Record<string, string> = {
  legal_name: 'legalName',
  tax_id: 'taxId',
  vat_ipn: 'vatIpn',
  bank_name: 'bankName',
  default_currency: 'defaultCurrency',
  default_discount_bp: 'defaultDiscountPercent',
  payment_terms_days: 'paymentTermsDays',
  'legal_address.country': 'legalAddress.country',
  'legal_address.street': 'legalAddress.street',
  'legal_address.zip': 'legalAddress.zip',
  'legal_address.city': 'legalAddress.city',
  'delivery_address.country': 'deliveryAddress.country',
  'delivery_address.street': 'deliveryAddress.street',
  'delivery_address.zip': 'deliveryAddress.zip',
  'delivery_address.city': 'deliveryAddress.city',
  'credit_limit.amount_minor': 'creditLimitAmount',
  'credit_limit.currency': 'creditLimitCurrency',
}

const CONTACT_FIELD_PATTERN = /^contacts\[(\d+)\]\.(.+)$/

/**
 * Translates a backend validation error key to the Mantine form path it
 * corresponds to, e.g. `contacts[0].email` -> `contacts.0.email`,
 * `legal_address.country` -> `legalAddress.country`. Money fields
 * (`credit_limit.*`) collapse onto the single decimal-string form field
 * they came from, mirroring how the order form handles `line_items[i].unit_price.*`.
 */
export function mapApiErrorField(field: string): string {
  const contactMatch = field.match(CONTACT_FIELD_PATTERN)
  if (contactMatch) {
    const [, index, rest] = contactMatch
    const restField = rest === 'is_primary' ? 'isPrimary' : rest
    return `contacts.${index}.${restField}`
  }
  return TOP_LEVEL_FIELD_MAP[field] ?? field
}
