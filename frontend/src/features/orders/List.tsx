import { useMemo, useState } from 'react'
import { Button, Group, Select, Stack, Table, Text, TextInput, Title } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { ListPagination } from '../../components/ListPagination'
import {
  StatusMark,
  StatusTag,
  renderStatusSelectOption,
  statusMetaFor,
} from '../../components/StatusBadge'
import { useApi } from '../../lib/api'
import { formatDateTime } from '../../lib/dates'
import { formatMoney } from '../../lib/money'
import { columnAlign } from '../../lib/table'
import { fetchOrders, ordersKeys } from './api'
import { CustomerSelect } from './CustomerSelect'
import type { Order, OrderStatusId } from './types'
import { useOrderStatusDictionary } from './useOrderStatusDictionary'

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
  const { t, i18n } = useTranslation('orders')
  const navigate = useNavigate()
  const api = useApi()
  const statusDict = useOrderStatusDictionary()
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
      status: status === null ? undefined : (Number(status) as OrderStatusId),
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
      columnHelper.accessor('status', {
        id: 'status',
        header: t('fields.status'),
        enableSorting: false,
        cell: (info) => {
          const statusId = info.getValue()
          const meta = statusDict.byId.get(statusId)
          return <StatusTag color={meta?.color} label={statusDict.labelFor(statusId)} />
        },
      }),
      columnHelper.accessor('number', { header: t('fields.number') }),
      columnHelper.accessor((row) => row.customer_name ?? row.customer_id, {
        id: 'customer_name',
        header: t('fields.customer'),
        enableSorting: false,
      }),
      columnHelper.accessor((row) => formatMoney(row.total, i18n.language), {
        id: 'total',
        header: t('fields.total'),
        enableSorting: false,
        meta: { align: 'right' },
      }),
      columnHelper.accessor('created_at', {
        header: t('fields.createdAt'),
        cell: (info) => formatDateTime(info.getValue(), i18n.language),
      }),
    ],
    [t, i18n.language, statusDict],
  )

  const selectedStatus = statusMetaFor(statusDict.byId, status)

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

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{t('list.title')}</Title>
        <Group gap="sm">
          <ListPagination page={page} pageSize={PAGE_SIZE} total={data?.total ?? 0} onChange={setPage} />
          <Button onClick={() => navigate({ to: '/orders/new' })}>{t('list.new')}</Button>
        </Group>
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
        <CustomerSelect
          label={null}
          required={false}
          placeholder={t('list.filterCustomer')}
          value={customerId}
          onChange={(next) => {
            setCustomerId(next)
            setPage(1)
          }}
        />
        <Select
          placeholder={t('list.filterStatus')}
          clearable
          data={statusDict.options}
          value={status}
          onChange={(value) => {
            setStatus(value)
            setPage(1)
          }}
          renderOption={renderStatusSelectOption(statusDict.byId)}
          leftSection={
            status != null ? (
              <StatusMark
                statusKey={selectedStatus.key}
                color={selectedStatus.color}
                label={statusDict.labelFor(Number(status) as OrderStatusId)}
                size={18}
                withTooltip={false}
                variant="filled"
              />
            ) : undefined
          }
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
                    ta={columnAlign(header.column)}
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
                <Table.Td key={cell.id} ta={columnAlign(cell.column)}>
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </Table.Td>
              ))}
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {!isLoading && data?.items.length === 0 && <Text c="dimmed">{t('list.empty')}</Text>}
    </Stack>
  )
}
