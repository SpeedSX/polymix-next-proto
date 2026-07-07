import { useMemo, useState } from 'react'
import { Badge, Button, Group, Pagination, Select, Stack, Table, Text, TextInput, Title } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { formatMoney } from '../../lib/money'
import { fetchOrders, ordersKeys } from './api'
import { ORDER_STATUSES } from './types'
import type { Order, OrderStatus } from './types'

const PAGE_SIZE = 25
const DEFAULT_SORT = '-created_at'
const SEARCH_DEBOUNCE_MS = 250

function sortParam(sorting: SortingState): string {
  if (sorting.length === 0) {
    return DEFAULT_SORT
  }
  const [{ id, desc }] = sorting
  return desc ? `-${id}` : id
}

const columnHelper = createColumnHelper<Order>()

export function OrderList() {
  const { t } = useTranslation('orders')
  const navigate = useNavigate()
  const api = useApi()
  const [page, setPage] = useState(1)
  const [sorting, setSorting] = useState<SortingState>([{ id: 'created_at', desc: true }])
  const [customerId, setCustomerId] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [debouncedSearch] = useDebouncedValue(search, SEARCH_DEBOUNCE_MS)
  const hasSearch = debouncedSearch.trim() !== ''

  const params = useMemo(
    () => ({
      page,
      limit: PAGE_SIZE,
      sort: sortParam(sorting),
      customer_id: customerId || undefined,
      status: (status as OrderStatus | null) ?? undefined,
      q: hasSearch ? debouncedSearch.trim() : undefined,
    }),
    [page, sorting, customerId, status, hasSearch, debouncedSearch],
  )

  const { data, isLoading, isError } = useQuery({
    queryKey: ordersKeys.list(params),
    queryFn: () => fetchOrders(api, params),
  })

  const columns = useMemo(
    () => [
      columnHelper.accessor('number', { header: t('fields.number') }),
      columnHelper.accessor('customer_id', { header: t('fields.customer') }),
      columnHelper.accessor('status', {
        header: t('fields.status'),
        cell: (info) => <Badge>{t(`status.${info.getValue()}`)}</Badge>,
      }),
      columnHelper.accessor((row) => formatMoney(row.total), {
        id: 'total',
        header: t('fields.total'),
        enableSorting: false,
      }),
    ],
    [t],
  )

  const handleSortingChange = (updaterOrValue: Updater<SortingState>) => {
    const next = typeof updaterOrValue === 'function' ? updaterOrValue(sorting) : updaterOrValue
    setSorting(next)
    setPage(1)
  }

  const table = useReactTable({
    data: data?.items ?? [],
    columns,
    state: { sorting },
    manualSorting: true,
    manualPagination: true,
    enableMultiSort: false,
    // The server ranks by BM25 score and ignores `sort` while a search
    // query is active — don't offer column sorting that would be a no-op.
    enableSorting: !hasSearch,
    onSortingChange: handleSortingChange,
    getCoreRowModel: getCoreRowModel(),
  })

  const totalPages = data ? Math.max(1, Math.ceil(data.total / PAGE_SIZE)) : 1

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{t('list.title')}</Title>
        <Button onClick={() => navigate({ to: '/orders/new' })}>{t('list.new')}</Button>
      </Group>
      <Group>
        <TextInput
          placeholder={t('list.searchPlaceholder')}
          value={search}
          onChange={(event) => {
            setSearch(event.currentTarget.value)
            setPage(1)
          }}
        />
        <TextInput
          placeholder={t('list.filterCustomer')}
          value={customerId}
          onChange={(event) => {
            setCustomerId(event.currentTarget.value)
            setPage(1)
          }}
        />
        <Select
          placeholder={t('list.filterStatus')}
          clearable
          data={ORDER_STATUSES.map((value) => ({ value, label: t(`status.${value}`) }))}
          value={status}
          onChange={(value) => {
            setStatus(value)
            setPage(1)
          }}
        />
      </Group>
      {isError && <Text c="red">{t('list.loadError')}</Text>}
      <Table highlightOnHover>
        <Table.Thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <Table.Tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => {
                const sortDirection = header.column.getIsSorted()
                return (
                  <Table.Th
                    key={header.id}
                    onClick={header.column.getToggleSortingHandler()}
                    style={{ cursor: header.column.getCanSort() ? 'pointer' : undefined }}
                  >
                    {flexRender(header.column.columnDef.header, header.getContext())}
                    {sortDirection === 'asc' && ' ▲'}
                    {sortDirection === 'desc' && ' ▼'}
                  </Table.Th>
                )
              })}
            </Table.Tr>
          ))}
        </Table.Thead>
        <Table.Tbody>
          {table.getRowModel().rows.map((row) => (
            <Table.Tr
              key={row.id}
              onClick={() => navigate({ to: '/orders/$id', params: { id: row.original.id } })}
              style={{ cursor: 'pointer' }}
            >
              {row.getVisibleCells().map((cell) => (
                <Table.Td key={cell.id}>{flexRender(cell.column.columnDef.cell, cell.getContext())}</Table.Td>
              ))}
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {!isLoading && data?.items.length === 0 && <Text c="dimmed">{t('list.empty')}</Text>}
      <Pagination value={page} onChange={setPage} total={totalPages} />
    </Stack>
  )
}
