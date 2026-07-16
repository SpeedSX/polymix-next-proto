import { useMemo } from 'react'
import { Anchor, Badge, Group, Loader, Paper, SimpleGrid, Stack, Table, Text, Title } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { formatDateTime } from '../../lib/dates'
import { formatMoney } from '../../lib/money'
import { fetchCustomerActivity, customersKeys } from './api'
import { fetchOrders, ordersKeys } from '../orders/api'
import { ORDER_STATUS } from '../orders/types'
import type { MonthlyOrderCount } from '../orders/types'
import { useOrderStatusDictionary } from '../orders/useOrderStatusDictionary'

const RECENT_ORDERS_LIMIT = 5
const MS_PER_DAY = 1000 * 60 * 60 * 24

function daysSince(iso: string, now: Date): number | null {
  const then = new Date(iso)
  if (Number.isNaN(then.getTime())) {
    return null
  }
  return Math.max(0, Math.floor((now.getTime() - then.getTime()) / MS_PER_DAY))
}

function Sparkline({ data, label }: { data: MonthlyOrderCount[]; label: string }) {
  const max = Math.max(1, ...data.map((point) => point.count))
  return (
    <Group gap={3} align="flex-end" h={40} role="img" aria-label={label}>
      {data.map((point) => (
        <Badge
          key={point.month}
          variant="light"
          radius="sm"
          p={0}
          w={10}
          h={`${Math.max(6, Math.round((point.count / max) * 40))}px`}
          title={`${point.month}: ${point.count}`}
        />
      ))}
    </Group>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <Paper withBorder p="sm" radius="md">
      <Text size="xs" c="dimmed">
        {label}
      </Text>
      <Text fw={600} size="lg">
        {value}
      </Text>
    </Paper>
  )
}

export function CustomerActivityPanel({ customerId }: { customerId: string }) {
  const { t, i18n } = useTranslation('customers')
  const api = useApi()
  const navigate = useNavigate()
  const statusDict = useOrderStatusDictionary()

  const { data: activity, isLoading, isError } = useQuery({
    queryKey: customersKeys.activity(customerId),
    queryFn: () => fetchCustomerActivity(api, customerId),
  })

  const recentParams = useMemo(
    () => ({ page: 1, limit: RECENT_ORDERS_LIMIT, sort: '-created_at', customer_id: customerId }),
    [customerId],
  )
  const { data: recent } = useQuery({
    queryKey: ordersKeys.list(recentParams),
    queryFn: () => fetchOrders(api, recentParams),
    enabled: !!activity && activity.total_orders > 0,
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !activity) {
    return <Text c="red">{t('activity.loadError')}</Text>
  }

  if (activity.total_orders === 0) {
    return (
      <Stack gap="xs">
        <Title order={4}>{t('sections.activity')}</Title>
        <Text c="dimmed">{t('activity.empty')}</Text>
      </Stack>
    )
  }

  const countFor = (status: number) =>
    activity.status_counts.find((entry) => entry.status === status)?.count ?? 0
  const completed = countFor(ORDER_STATUS.Completed)
  const drafts = countFor(ORDER_STATUS.Draft)
  const draftRatio = Math.round((drafts / activity.total_orders) * 100)
  const days = activity.last_order_at ? daysSince(activity.last_order_at, new Date()) : null

  return (
    <Stack gap="sm">
      <Title order={4}>{t('sections.activity')}</Title>

      <SimpleGrid cols={{ base: 2, sm: 3, md: 5 }}>
        <Stat label={t('activity.totalOrders')} value={String(activity.total_orders)} />
        <Stat label={t('activity.completedOrders')} value={String(completed)} />
        <Stat label={t('activity.draftRatio')} value={`${draftRatio}%`} />
        <Stat label={t('activity.totalSpend')} value={formatMoney(activity.total_spend, i18n.language)} />
        <Stat label={t('activity.ordersLast30Days')} value={String(activity.orders_last_30_days)} />
      </SimpleGrid>

      {activity.last_order_at && (
        <Text size="sm" c="dimmed">
          {t('activity.lastOrder', {
            date: formatDateTime(activity.last_order_at, i18n.language),
            count: days ?? 0,
          })}
        </Text>
      )}

      <Group gap="xs">
        {activity.status_counts.map((entry) => (
          <Badge key={entry.status} color={statusDict.byId.get(entry.status)?.color} variant="light">
            {statusDict.labelFor(entry.status)}: {entry.count}
          </Badge>
        ))}
      </Group>

      <Stack gap={4}>
        <Text size="xs" c="dimmed">
          {t('activity.ordersByMonth')}
        </Text>
        <Sparkline data={activity.orders_by_month} label={t('activity.ordersByMonth')} />
      </Stack>

      {recent && recent.items.length > 0 && (
        <Stack gap="xs">
          <Text size="sm" fw={600}>
            {t('activity.recentOrders')}
          </Text>
          <Table highlightOnHover>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>{t('activity.orderNumber')}</Table.Th>
                <Table.Th>{t('fields.status')}</Table.Th>
                <Table.Th>{t('activity.orderTotal')}</Table.Th>
                <Table.Th>{t('activity.orderDate')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {recent.items.map((order) => (
                <Table.Tr key={order.id}>
                  <Table.Td>
                    <Anchor onClick={() => navigate({ to: '/orders/$id', params: { id: order.id } })}>
                      {order.number}
                    </Anchor>
                  </Table.Td>
                  <Table.Td>
                    <Badge color={statusDict.byId.get(order.status)?.color} variant="light">
                      {statusDict.labelFor(order.status)}
                    </Badge>
                  </Table.Td>
                  <Table.Td>{formatMoney(order.total, i18n.language)}</Table.Td>
                  <Table.Td>{formatDateTime(order.created_at, i18n.language)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Stack>
      )}
    </Stack>
  )
}
