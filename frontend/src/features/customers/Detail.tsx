import { Alert, Button, Group, Loader, Stack, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { customersKeys, deleteCustomer, fetchCustomer, updateCustomer } from './api'
import { CustomerForm } from './Form'
import { fromCustomer } from './types'

export function CustomerDetail() {
  const { t } = useTranslation('customers')
  const { id } = useParams({ from: '/customers/$id' })
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()

  const {
    data: customer,
    isLoading,
    isError,
  } = useQuery({
    queryKey: customersKeys.detail(id),
    queryFn: () => fetchCustomer(api, id),
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteCustomer(api, id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: customersKeys.all })
      void navigate({ to: '/customers' })
    },
  })

  if (isLoading) {
    return <Loader />
  }

  if (isError || !customer) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{customer.name}</Title>
        <Button
          color="red"
          variant="outline"
          onClick={() => deleteMutation.mutate()}
          loading={deleteMutation.isPending}
        >
          {t('detail.delete')}
        </Button>
      </Group>
      {deleteMutation.isError && <Alert color="red">{t('detail.deleteError')}</Alert>}
      <CustomerForm
        initialValues={fromCustomer(customer)}
        onSubmit={(data) => updateCustomer(api, id, data)}
        onSuccess={(updated) => {
          queryClient.setQueryData(customersKeys.detail(id), updated)
          void queryClient.invalidateQueries({ queryKey: customersKeys.all })
        }}
        onCancel={() => navigate({ to: '/customers' })}
      />
    </Stack>
  )
}
