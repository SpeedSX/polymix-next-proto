import { useState } from 'react'
import { Alert, Badge, Button, Group, Loader, Stack, Table, Text, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { formatDate } from '../../lib/dates'
import { convertedDisplay, formatMoney } from '../../lib/money'
import { fetchInvoice, invoicesKeys, setInvoiceStatus, updateInvoice } from './api'
import { InvoiceForm } from './Form'
import { fromInvoice, INVOICE_TRANSITIONS } from './types'
import type { Invoice, InvoiceStatus, UpdateInvoice } from './types'

interface MeResponse {
  tenant: { default_currency: string }
}

export function InvoiceDetail() {
  const { t, i18n } = useTranslation('invoices')
  const { id } = useParams({ from: '/invoices/$id' })
  const api = useApi()
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const { data: invoice, isLoading, isError } = useQuery({
    queryKey: invoicesKeys.detail(id),
    queryFn: () => fetchInvoice(api, id),
  })
  const { data: me } = useQuery({
    queryKey: ['me'],
    queryFn: () => api<MeResponse>('/api/me'),
  })

  const statusMutation = useMutation({
    mutationFn: (status: InvoiceStatus) => setInvoiceStatus(api, id, status),
    onMutate: async (status) => {
      await queryClient.cancelQueries({ queryKey: invoicesKeys.detail(id) })
      const previous = queryClient.getQueryData<Invoice>(invoicesKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Invoice>(invoicesKeys.detail(id), { ...previous, status })
      }
      return { previous }
    },
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(invoicesKeys.detail(id), updated)
    },
    onError: (err, _status, context) => {
      if (context?.previous) {
        queryClient.setQueryData(invoicesKeys.detail(id), context.previous)
      }
      if (err instanceof ApiError && err.code === 'invoice_status_transition' && err.details) {
        setActionError(
          t('errors.invoice_status_transition', {
            from: t(`status.${String(err.details.from)}`),
            to: t(`status.${String(err.details.to)}`),
          }),
        )
      } else {
        setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: invoicesKeys.all }),
  })

  const updateMutation = useMutation({
    mutationFn: (data: UpdateInvoice) => updateInvoice(api, id, data),
    onMutate: async (data) => {
      await queryClient.cancelQueries({ queryKey: invoicesKeys.detail(id) })
      const previous = queryClient.getQueryData<Invoice>(invoicesKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Invoice>(invoicesKeys.detail(id), { ...previous, ...data })
      }
      return { previous }
    },
    onSuccess: (updated) => queryClient.setQueryData(invoicesKeys.detail(id), updated),
    onError: (_err, _data, context) => {
      if (context?.previous) {
        queryClient.setQueryData(invoicesKeys.detail(id), context.previous)
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: invoicesKeys.all }),
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !invoice) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  if (editing) {
    return (
      <Stack>
        <Title order={2}>{invoice.number}</Title>
        <InvoiceForm
          initialValues={fromInvoice(invoice, i18n.language)}
          currency={invoice.currency}
          onSubmit={(data) => updateMutation.mutateAsync(data)}
          onSuccess={() => setEditing(false)}
          onCancel={() => setEditing(false)}
        />
      </Stack>
    )
  }

  const nextStatuses = INVOICE_TRANSITIONS[invoice.status]

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{invoice.number}</Title>
        <Badge>{t(`status.${invoice.status}`)}</Badge>
      </Group>
      {actionError && <Alert color="red">{actionError}</Alert>}
      <Text>
        {t('fields.order')}: {invoice.order_id}
      </Text>
      <Text>
        {t('fields.customer')}: {invoice.customer_id}
      </Text>
      {invoice.issue_date && (
        <Text>
          {t('fields.issueDate')}: {formatDate(invoice.issue_date, i18n.language)}
        </Text>
      )}
      {invoice.due_date && (
        <Text>
          {t('fields.dueDate')}: {formatDate(invoice.due_date, i18n.language)}
        </Text>
      )}

      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>{t('fields.description')}</Table.Th>
            <Table.Th>{t('fields.quantity')}</Table.Th>
            <Table.Th>{t('fields.unitPrice')}</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {invoice.line_items.map((item, index) => (
            <Table.Tr key={index}>
              <Table.Td>{item.description}</Table.Td>
              <Table.Td>{item.quantity}</Table.Td>
              <Table.Td>{formatMoney(item.unit_price, i18n.language)}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Stack gap={4}>
        <Text>
          {t('fields.netTotal')}: {formatMoney(invoice.net_total, i18n.language)}
        </Text>
        <Text>
          {t('fields.taxTotal')}: {formatMoney(invoice.tax_total, i18n.language)} ({invoice.tax_rate_bp / 100}%)
        </Text>
        <Text fw={600}>
          {t('fields.grossTotal')}: {formatMoney(invoice.gross_total, i18n.language)}
        </Text>
        {invoice.exchange_rate && me && (
          <Text c="dimmed" size="sm">
            {t('detail.convertedGrossTotal', {
              amount: convertedDisplay(invoice.gross_total, invoice.exchange_rate, me.tenant.default_currency, i18n.language),
            })}
          </Text>
        )}
      </Stack>

      <Group>
        {nextStatuses.map((next) => (
          <Button
            key={next}
            variant="light"
            loading={statusMutation.isPending}
            onClick={() => statusMutation.mutate(next)}
          >
            {t('actions.transitionTo', { status: t(`status.${next}`) })}
          </Button>
        ))}
        {invoice.status === 'draft' && (
          <Button variant="subtle" onClick={() => setEditing(true)}>
            {t('form.edit')}
          </Button>
        )}
      </Group>
    </Stack>
  )
}
