import { useState } from 'react'
import { Alert, Button, Group, Loader, Stack, Table, Text } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { PageHeader } from '../../components/PageHeader'
import { StatusTag } from '../../components/StatusBadge'
import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { formatMoney } from '../../lib/money'
import { createInvoiceFromOrder, deleteOrder, fetchOrder, ordersKeys, setOrderStatus, updateOrder } from './api'
import { OrderForm } from './Form'
import { orderStatusTone } from './statusTone'
import { fromOrder, ORDER_STATUS } from './types'
import type { NewOrder, Order, OrderStatusId } from './types'
import { useOrderStatusDictionary } from './useOrderStatusDictionary'

export function OrderDetail() {
  const { t, i18n } = useTranslation('orders')
  const { id } = useParams({ from: '/orders/$id' })
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const statusDict = useOrderStatusDictionary()
  const [editing, setEditing] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const { data: order, isLoading, isError } = useQuery({
    queryKey: ordersKeys.detail(id),
    queryFn: () => fetchOrder(api, id),
  })

  const statusMutation = useMutation({
    mutationFn: (status: OrderStatusId) => setOrderStatus(api, id, status),
    onMutate: async (status) => {
      await queryClient.cancelQueries({ queryKey: ordersKeys.detail(id) })
      const previous = queryClient.getQueryData<Order>(ordersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Order>(ordersKeys.detail(id), { ...previous, status })
      }
      return { previous }
    },
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(ordersKeys.detail(id), updated)
    },
    onError: (err, _status, context) => {
      if (context?.previous) {
        queryClient.setQueryData(ordersKeys.detail(id), context.previous)
      }
      if (err instanceof ApiError && err.code === 'order_status_transition' && err.details) {
        const from = Number(err.details.from) as OrderStatusId
        const to = Number(err.details.to) as OrderStatusId
        setActionError(
          t('errors.order_status_transition', {
            from: statusDict.labelFor(from),
            to: statusDict.labelFor(to),
          }),
        )
      } else {
        setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ordersKeys.all }),
  })

  const updateMutation = useMutation({
    mutationFn: (data: NewOrder) => updateOrder(api, id, data),
    onMutate: async (data) => {
      await queryClient.cancelQueries({ queryKey: ordersKeys.detail(id) })
      const previous = queryClient.getQueryData<Order>(ordersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Order>(ordersKeys.detail(id), { ...previous, ...data })
      }
      return { previous }
    },
    onSuccess: (updated) => queryClient.setQueryData(ordersKeys.detail(id), updated),
    onError: (_err, _data, context) => {
      if (context?.previous) {
        queryClient.setQueryData(ordersKeys.detail(id), context.previous)
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ordersKeys.all }),
  })

  const invoiceMutation = useMutation({
    mutationFn: () => createInvoiceFromOrder(api, id),
    onSuccess: (invoice) => {
      setActionError(null)
      void navigate({ to: '/invoices/$id', params: { id: invoice.id } })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'form.unexpectedError')),
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteOrder(api, id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ordersKeys.all })
      void navigate({ to: '/orders' })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'detail.deleteError')),
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !order) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  const meta = statusDict.byId.get(order.status)

  if (editing) {
    return (
      <OrderForm
        breadcrumb={[t('list.title'), t('form.edit')]}
        title={order.number}
        status={<StatusTag tone={orderStatusTone(meta?.key)} label={statusDict.labelFor(order.status)} />}
        initialValues={fromOrder(order, i18n.language)}
        onSubmit={(data) => updateMutation.mutateAsync(data)}
        onSuccess={() => setEditing(false)}
        onCancel={() => setEditing(false)}
      />
    )
  }

  const nextStatuses = meta?.allowed_targets ?? []
  const canInvoice = meta?.invoiceable ?? false

  return (
    <Stack>
      <PageHeader
        breadcrumb={[t('list.title')]}
        title={order.number}
        status={<StatusTag tone={orderStatusTone(meta?.key)} label={statusDict.labelFor(order.status)} />}
        actions={
          <>
            {order.status === ORDER_STATUS.Draft && (
              <Button variant="subtle" onClick={() => setEditing(true)}>
                {t('form.edit')}
              </Button>
            )}
            <Button
              color="red"
              variant="subtle"
              loading={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate()}
            >
              {t('detail.delete')}
            </Button>
          </>
        }
      />
      {actionError && <Alert color="red">{actionError}</Alert>}
      <Text>
        {t('fields.customer')}: {order.customer_name ?? order.customer_id}
      </Text>

      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>{t('fields.description')}</Table.Th>
            <Table.Th ta="right">{t('fields.quantity')}</Table.Th>
            <Table.Th ta="right">{t('fields.unitPrice')}</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {order.line_items.map((item, index) => (
            <Table.Tr key={index}>
              <Table.Td>{item.description}</Table.Td>
              <Table.Td ta="right">{item.quantity}</Table.Td>
              <Table.Td ta="right">{formatMoney(item.unit_price, i18n.language)}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      <Text fw={500}>
        {t('fields.total')}: {formatMoney(order.total, i18n.language)}
      </Text>

      {(nextStatuses.length > 0 || canInvoice) && (
        <Group>
          {nextStatuses.map((next) => (
            <Button
              key={next}
              variant="light"
              loading={statusMutation.isPending}
              onClick={() => statusMutation.mutate(next)}
            >
              {t(`actions.transitionTo`, { status: statusDict.labelFor(next) })}
            </Button>
          ))}
          {canInvoice && (
            <Button loading={invoiceMutation.isPending} onClick={() => invoiceMutation.mutate()}>
              {t('actions.generateInvoice')}
            </Button>
          )}
        </Group>
      )}
    </Stack>
  )
}
