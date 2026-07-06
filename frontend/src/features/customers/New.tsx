import { Stack, Title } from '@mantine/core'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { createCustomer, customersKeys } from './api'
import { CustomerForm } from './Form'
import { emptyCustomerFormValues } from './types'

export function CustomerNew() {
  const { t } = useTranslation('customers')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()

  return (
    <Stack>
      <Title order={2}>{t('create.title')}</Title>
      <CustomerForm
        initialValues={emptyCustomerFormValues}
        onSubmit={(data) => createCustomer(api, data)}
        onSuccess={() => {
          void queryClient.invalidateQueries({ queryKey: customersKeys.all })
          void navigate({ to: '/customers' })
        }}
        onCancel={() => navigate({ to: '/customers' })}
      />
    </Stack>
  )
}
