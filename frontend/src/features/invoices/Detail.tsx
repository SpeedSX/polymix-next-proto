import { useState } from 'react'
import { Alert, Badge, Button, Group, Loader, Stack, Table, Text, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ApiError, useApi } from '../../lib/api'
import { formatMoney } from '../../lib/money'
import { fetchInvoice, invoicesKeys, setInvoiceStatus } from './api'
import { INVOICE_TRANSITIONS } from './types'
import type { InvoiceStatus } from './types'

export function InvoiceDetail() {
  const { t } = useTranslation('invoices')
  const { id } = useParams({ from: '/invoices/$id' })
  const api = useApi()
  const queryClient = useQueryClient()
  const [actionError, setActionError] = useState<string | null>(null)

  const { data: invoice, isLoading, isError } = useQuery({
    queryKey: invoicesKeys.detail(id),
    queryFn: () => fetchInvoice(api, id),
  })

  const statusMutation = useMutation({
    mutationFn: (status: InvoiceStatus) => setInvoiceStatus(api, id, status),
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(invoicesKeys.detail(id), updated)
      void queryClient.invalidateQueries({ queryKey: invoicesKeys.all })
    },
    onError: (err) => setActionError(err instanceof ApiError ? err.message : t('form.unexpectedError')),
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !invoice) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
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
          {t('fields.issueDate')}: {invoice.issue_date}
        </Text>
      )}
      {invoice.due_date && (
        <Text>
          {t('fields.dueDate')}: {invoice.due_date}
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
              <Table.Td>{formatMoney(item.unit_price)}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Stack gap={4}>
        <Text>
          {t('fields.netTotal')}: {formatMoney(invoice.net_total)}
        </Text>
        <Text>
          {t('fields.taxTotal')}: {formatMoney(invoice.tax_total)} ({invoice.tax_rate_bp / 100}%)
        </Text>
        <Text fw={600}>
          {t('fields.grossTotal')}: {formatMoney(invoice.gross_total)}
        </Text>
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
      </Group>
    </Stack>
  )
}
