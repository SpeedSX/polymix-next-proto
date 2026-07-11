import { Alert, Button, Group, Loader, Stack, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { apiErrorMessage, useApi } from '../../lib/api'
import { customersKeys, deleteCustomer, fetchCustomer, updateCustomer } from './api'
import { CustomerForm } from './Form'
import { fromCustomer } from './types'
import type { Customer, NewCustomer } from './types'

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

  const updateMutation = useMutation({
    mutationFn: (data: NewCustomer) => updateCustomer(api, id, data),
    onMutate: async (data) => {
      await queryClient.cancelQueries({ queryKey: customersKeys.detail(id) })
      const previous = queryClient.getQueryData<Customer>(customersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Customer>(customersKeys.detail(id), { ...previous, ...data })
      }
      return { previous }
    },
    onSuccess: (updated) => queryClient.setQueryData(customersKeys.detail(id), updated),
    onError: (_err, _data, context) => {
      if (context?.previous) {
        queryClient.setQueryData(customersKeys.detail(id), context.previous)
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: customersKeys.all }),
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
      {deleteMutation.isError && (
        <Alert color="red">{apiErrorMessage(deleteMutation.error, t, 'detail.deleteError')}</Alert>
      )}
      <CustomerForm
        initialValues={fromCustomer(customer)}
        onSubmit={(data) => updateMutation.mutateAsync(data)}
        onSuccess={() => void navigate({ to: '/customers' })}
        onCancel={() => navigate({ to: '/customers' })}
      />
    </Stack>
  )
}
