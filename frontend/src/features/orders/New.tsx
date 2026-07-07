import { Loader, Stack, Title } from '@mantine/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { createOrder, ordersKeys } from './api'
import { OrderForm } from './Form'
import { emptyOrderFormValues } from './types'

interface MeResponse {
  tenant: { default_currency: string }
}

export function OrderNew() {
  const { t } = useTranslation('orders')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()

  const { data: me, isLoading } = useQuery({
    queryKey: ['me'],
    queryFn: () => api<MeResponse>('/api/me'),
  })

  if (isLoading || !me) {
    return <Loader />
  }

  return (
    <Stack>
      <Title order={2}>{t('create.title')}</Title>
      <OrderForm
        initialValues={emptyOrderFormValues(me.tenant.default_currency)}
        onSubmit={(data) => createOrder(api, data)}
        onSuccess={(order) => {
          void queryClient.invalidateQueries({ queryKey: ordersKeys.all })
          void navigate({ to: '/orders/$id', params: { id: order.id } })
        }}
        onCancel={() => navigate({ to: '/orders' })}
      />
    </Stack>
  )
}
