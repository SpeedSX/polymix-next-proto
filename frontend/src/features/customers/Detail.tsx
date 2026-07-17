import { useState } from 'react'
import { Alert, Button, Divider, Loader, Stack } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { StatusTag } from '../../components/StatusBadge'
import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { CustomerActivityPanel } from './Activity'
import { customersKeys, deleteCustomer, fetchCustomer, setCustomerStatus, updateCustomer } from './api'
import { CustomerForm } from './Form'
import { fromCustomer } from './types'
import type { Customer, CustomerStatusId, NewCustomer } from './types'
import { useCustomerStatusDictionary } from './useCustomerStatusDictionary'

export function CustomerDetail() {
  const { t } = useTranslation('customers')
  const { id } = useParams({ from: '/customers/$id' })
  const queryClient = useQueryClient()
  const api = useApi()
  // Bumped after a conflict-reload so the editor remounts on the refreshed
  // data and re-snapshots its optimistic-concurrency version.
  const [reloadNonce, setReloadNonce] = useState(0)

  const { data: customer, isLoading, isError } = useQuery({
    queryKey: customersKeys.detail(id),
    queryFn: () => fetchCustomer(api, id),
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !customer) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  return (
    <CustomerEditor
      key={reloadNonce}
      customer={customer}
      onReload={async () => {
        await queryClient.invalidateQueries({ queryKey: customersKeys.detail(id) })
        setReloadNonce((nonce) => nonce + 1)
      }}
    />
  )
}

function CustomerEditor({ customer, onReload }: { customer: Customer; onReload: () => void }) {
  const { t, i18n } = useTranslation('customers')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const statusDict = useCustomerStatusDictionary()
  const [actionError, setActionError] = useState<string | null>(null)
  const id = customer.id
  // Snapshot of the version at mount — the optimistic-concurrency token. It
  // must NOT be re-read from the query cache at save time: a live WS update
  // from another user's write refreshes that cache to the new server version,
  // which would make a stale editor's guard pass and silently clobber.
  // Freezing it here means a concurrent change instead trips the 409 → reload
  // path. It only advances on our own successful save.
  const [baseVersion, setBaseVersion] = useState(customer.version)

  const statusMutation = useMutation({
    mutationFn: (status: CustomerStatusId) => setCustomerStatus(api, id, status),
    onMutate: async (status) => {
      await queryClient.cancelQueries({ queryKey: customersKeys.detail(id) })
      const previous = queryClient.getQueryData<Customer>(customersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Customer>(customersKeys.detail(id), { ...previous, status })
      }
      return { previous }
    },
    onSuccess: (updated) => {
      setActionError(null)
      setBaseVersion(updated.version)
      queryClient.setQueryData(customersKeys.detail(id), updated)
    },
    onError: (err, _status, context) => {
      if (context?.previous) {
        queryClient.setQueryData(customersKeys.detail(id), context.previous)
      }
      if (err instanceof ApiError && err.code === 'customer_status_transition' && err.details) {
        const from = Number(err.details.from) as CustomerStatusId
        const to = Number(err.details.to) as CustomerStatusId
        setActionError(
          t('errors.customer_status_transition', {
            from: statusDict.labelFor(from),
            to: statusDict.labelFor(to),
          }),
        )
      } else {
        setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: customersKeys.all }),
  })

  const updateMutation = useMutation({
    mutationFn: (data: NewCustomer) => updateCustomer(api, id, data, baseVersion),
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
    onError: (err) => setActionError(apiErrorMessage(err, t, 'detail.deleteError')),
  })

  const meta = statusDict.byId.get(customer.status)
  const nextStatuses = meta?.allowed_targets ?? []

  const sidePanel = (
    <Stack gap="lg">
      <CustomerActivityPanel customerId={id} compact />
      {actionError && <Alert color="red">{actionError}</Alert>}
      <Divider />
      <Stack gap="xs">
        {nextStatuses.map((next) => (
          <Button
            key={next}
            variant="light"
            fullWidth
            loading={statusMutation.isPending}
            onClick={() => statusMutation.mutate(next)}
          >
            {t('actions.transitionTo', { status: statusDict.labelFor(next) })}
          </Button>
        ))}
        <Button
          color="red"
          variant="subtle"
          fullWidth
          loading={deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          {t('detail.delete')}
        </Button>
      </Stack>
    </Stack>
  )

  return (
    <CustomerForm
      breadcrumb={[t('list.title'), t('form.edit')]}
      title={customer.name}
      status={<StatusTag color={meta?.color} label={statusDict.labelFor(customer.status)} />}
      initialValues={fromCustomer(customer, i18n.language)}
      onSubmit={(data) => updateMutation.mutateAsync(data)}
      onSuccess={(updated) => setBaseVersion(updated.version)}
      onCancel={() => navigate({ to: '/customers' })}
      onConflict={onReload}
      sidePanel={sidePanel}
    />
  )
}
