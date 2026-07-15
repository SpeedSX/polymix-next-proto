import { useState } from 'react'
import { Alert, Badge, Button, Group, Loader, Stack, Table, Text, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { customersKeys, deleteCustomer, fetchCustomer, setCustomerStatus, updateCustomer } from './api'
import { CustomerForm } from './Form'
import { fromCustomer } from './types'
import type { Customer, CustomerStatusId, NewCustomer } from './types'
import { useCustomerStatusDictionary } from './useCustomerStatusDictionary'

export function CustomerDetail() {
  const { t, i18n } = useTranslation('customers')
  const { id } = useParams({ from: '/customers/$id' })
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const statusDict = useCustomerStatusDictionary()
  const [editing, setEditing] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const {
    data: customer,
    isLoading,
    isError,
  } = useQuery({
    queryKey: customersKeys.detail(id),
    queryFn: () => fetchCustomer(api, id),
  })

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
    onError: (err) => setActionError(apiErrorMessage(err, t, 'detail.deleteError')),
  })

  if (isLoading) {
    return <Loader />
  }

  if (isError || !customer) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  if (editing) {
    return (
      <Stack>
        <Title order={2}>{customer.name}</Title>
        <CustomerForm
          initialValues={fromCustomer(customer, i18n.language)}
          onSubmit={(data) => updateMutation.mutateAsync(data)}
          onSuccess={() => setEditing(false)}
          onCancel={() => setEditing(false)}
        />
      </Stack>
    )
  }

  const meta = statusDict.byId.get(customer.status)
  const nextStatuses = meta?.allowed_targets ?? []
  const primaryContact = customer.contacts.find((contact) => contact.is_primary) ?? customer.contacts[0]

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{customer.name}</Title>
        <Badge color={meta?.color}>{statusDict.labelFor(customer.status)}</Badge>
      </Group>
      {actionError && <Alert color="red">{actionError}</Alert>}
      {customer.legal_name && <Text>{customer.legal_name}</Text>}
      {customer.edrpou && (
        <Text>
          {t('fields.edrpou')}: {customer.edrpou}
        </Text>
      )}
      {customer.tax_id && (
        <Text>
          {t('fields.taxId')}: {customer.tax_id}
        </Text>
      )}
      {primaryContact && (
        <Text>
          {t('fields.contactName')}: {primaryContact.name}
          {primaryContact.email ? ` (${primaryContact.email})` : ''}
        </Text>
      )}
      {customer.tags.length > 0 && (
        <Group gap="xs">
          {customer.tags.map((tag) => (
            <Badge key={tag} variant="light">
              {tag}
            </Badge>
          ))}
        </Group>
      )}

      {customer.contacts.length > 0 && (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>{t('fields.contactName')}</Table.Th>
              <Table.Th>{t('fields.contactRole')}</Table.Th>
              <Table.Th>{t('fields.email')}</Table.Th>
              <Table.Th>{t('fields.phone')}</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {customer.contacts.map((contact, index) => (
              <Table.Tr key={index}>
                <Table.Td>
                  {contact.name}
                  {contact.is_primary ? ` (${t('fields.primary')})` : ''}
                </Table.Td>
                <Table.Td>{contact.role}</Table.Td>
                <Table.Td>{contact.email}</Table.Td>
                <Table.Td>{contact.phone}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}

      <Group>
        {nextStatuses.map((next) => (
          <Button
            key={next}
            variant="light"
            loading={statusMutation.isPending}
            onClick={() => statusMutation.mutate(next)}
          >
            {t('actions.transitionTo', { status: statusDict.labelFor(next) })}
          </Button>
        ))}
        <Button variant="subtle" onClick={() => setEditing(true)}>
          {t('form.edit')}
        </Button>
        <Button
          color="red"
          variant="subtle"
          loading={deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          {t('detail.delete')}
        </Button>
      </Group>
    </Stack>
  )
}
