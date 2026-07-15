import { useState } from 'react'
import type { ReactNode } from 'react'
import { Select } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { fetchCustomer, fetchCustomers } from '../customers/api'
import { useCustomerStatusDictionary } from '../customers/useCustomerStatusDictionary'

const SEARCH_DEBOUNCE_MS = 250
const SEARCH_RESULT_LIMIT = 20

export interface CustomerSelectProps {
  value?: string
  onChange?: (customerId: string) => void
  error?: ReactNode
  onFocus?: () => void
  onBlur?: () => void
  label?: ReactNode
  placeholder?: string
  required?: boolean
  'data-path'?: string
}

/**
 * Searchable customer picker — resolves to a `customer_id`, backed by the
 * customer FTS list endpoint (`GET /api/customers?q=`) instead of manual id
 * entry (PLAN.md M4.1). Also resolves the current value's label via a detail
 * fetch so a pre-selected customer still shows a name even when it falls
 * outside the current search results. Used both for the order form's
 * required customer field and the orders list's optional customer filter.
 */
export function CustomerSelect({
  value = '',
  onChange,
  error,
  label,
  placeholder,
  required = true,
  ...rest
}: CustomerSelectProps) {
  const { t } = useTranslation('orders')
  const api = useApi()
  const statusDict = useCustomerStatusDictionary()
  const [search, setSearch] = useState('')
  const [debouncedSearch] = useDebouncedValue(search, SEARCH_DEBOUNCE_MS)

  const { data: searchResults } = useQuery({
    queryKey: ['customers', 'order-select-search', debouncedSearch],
    queryFn: () => fetchCustomers(api, { page: 1, limit: SEARCH_RESULT_LIMIT, sort: '-created_at', q: debouncedSearch || undefined }),
  })

  const { data: selectedCustomer } = useQuery({
    queryKey: ['customers', 'order-select-detail', value],
    queryFn: () => fetchCustomer(api, value),
    enabled: value !== '',
  })

  // Customers in a status that can't place an order (inactive/blocked) are
  // filtered out here; if a race lets one through anyway, order creation
  // still 409s and the caller surfaces that as a toast (see docs/customers-crm.md).
  const options = new Map<string, string>()
  for (const customer of searchResults?.items ?? []) {
    if (statusDict.byId.get(customer.status)?.can_order ?? true) {
      options.set(customer.id, customer.name)
    }
  }
  if (selectedCustomer && !options.has(selectedCustomer.id)) {
    options.set(selectedCustomer.id, selectedCustomer.name)
  }
  const data = Array.from(options, ([optionValue, label]) => ({ value: optionValue, label }))

  return (
    <Select
      label={label === undefined ? t('fields.customer') : label}
      withAsterisk={required}
      searchable
      clearable
      placeholder={placeholder ?? t('fields.customerSearchPlaceholder')}
      nothingFoundMessage={t('fields.customerNothingFound')}
      filter={({ options: opts }) => opts}
      searchValue={search}
      onSearchChange={setSearch}
      data={data}
      value={value || null}
      onChange={(next) => onChange?.(next ?? '')}
      error={error}
      {...rest}
    />
  )
}
