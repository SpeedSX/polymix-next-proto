import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { customersKeys, fetchCustomerStatusDictionary } from './api'
import type { CustomerStatusDictionaryItem, CustomerStatusId } from './types'

export function useCustomerStatusDictionary() {
  const api = useApi()
  const { i18n } = useTranslation('customers')

  const query = useQuery({
    queryKey: customersKeys.statusDictionary(),
    queryFn: () => fetchCustomerStatusDictionary(api),
    staleTime: Infinity,
  })

  const byId = useMemo(() => {
    const map = new Map<CustomerStatusId, CustomerStatusDictionaryItem>()
    for (const item of query.data?.items ?? []) {
      map.set(item.id, item)
    }
    return map
  }, [query.data])

  const labelFor = useMemo(() => {
    const lang = i18n.language
    return (id: CustomerStatusId): string => {
      const item = byId.get(id)
      if (!item) return String(id)
      return item.labels[lang] ?? item.labels.en ?? Object.values(item.labels)[0] ?? item.key
    }
  }, [byId, i18n.language])

  const options = useMemo(() => {
    const items = [...(query.data?.items ?? [])].sort((a, b) => (a.sort ?? 0) - (b.sort ?? 0))
    return items.map((item) => ({ value: String(item.id), label: labelFor(item.id) }))
  }, [query.data, labelFor])

  return {
    ...query,
    byId,
    labelFor,
    options,
  }
}
