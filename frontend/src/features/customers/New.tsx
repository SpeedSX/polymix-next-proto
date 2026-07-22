import { Alert, Loader } from '@mantine/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { createCustomer, customersKeys } from './api'
import { CustomerForm } from './Form'
import { emptyCustomerFormValues } from './types'

interface MeResponse {
  tenant: { default_currency: string }
}

export function CustomerNew() {
  const { t } = useTranslation('customers')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()

  const { data: me, isLoading, isError } = useQuery({
    queryKey: ['me'],
    queryFn: () => api<MeResponse>('/api/me'),
  })

  if (isError) {
    return <Alert color="red">{t('form.unexpectedError')}</Alert>
  }

  if (isLoading || !me) {
    return <Loader />
  }

  return (
    <CustomerForm
      breadcrumb={[t('list.title')]}
      title={t('create.title')}
      initialValues={emptyCustomerFormValues(me.tenant.default_currency)}
      onSubmit={(data) => createCustomer(api, data)}
      onSuccess={(customer) => {
        void queryClient.invalidateQueries({ queryKey: customersKeys.all })
        void navigate({ to: '/customers/$id', params: { id: customer.id } })
      }}
      onCancel={() => navigate({ to: '/customers' })}
    />
  )
}
