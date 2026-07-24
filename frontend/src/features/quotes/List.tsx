import { useMemo, useState } from 'react'
import { Button, Select, Table, Text } from '@mantine/core'
import { IconPlus } from '@tabler/icons-react'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { LIST_HEADER_HEIGHT, ListLayout } from '../../components/ListLayout'
import { ListPagination } from '../../components/ListPagination'
import { StatusTag } from '../../components/StatusBadge'
import { useApi } from '../../lib/api'
import { formatDate, formatDateTime } from '../../lib/dates'
import { formatMoney } from '../../lib/money'
import { columnAlign, columnWidth } from '../../lib/table'
import { CustomerSelect } from '../orders/CustomerSelect'
import { fetchQuotes, quotesKeys } from './api'
import { useQuoteStatus } from './useQuoteStatus'
import type { Quote, QuoteStatusId } from './types'

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

function partyLabel(quote: Quote): string {
  return quote.customer_name ?? quote.prospect?.name ?? quote.customer_id ?? '—'
}

const columnHelper = createColumnHelper<Quote>()

export function QuoteList() {
  const { t, i18n } = useTranslation('quotes')
  const navigate = useNavigate()
  const api = useApi()
  const status = useQuoteStatus()
  const [page, setPage] = useState(1)
  const [sorting, setSorting] = useState<SortingState>([{ id: 'created_at', desc: true }])
  const [customerId, setCustomerId] = useState('')
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const [debouncedSearch] = useDebouncedValue(search, SEARCH_DEBOUNCE_MS)
  const hasSearch = debouncedSearch.trim() !== ''

  const params = useMemo(
    () => ({
      page,
      limit: PAGE_SIZE,
      sort: sortParam(sorting),
      customer_id: customerId || undefined,
      status: statusFilter === null ? undefined : (Number(statusFilter) as QuoteStatusId),
      q: hasSearch ? debouncedSearch.trim() : undefined,
    }),
    [page, sorting, customerId, statusFilter, hasSearch, debouncedSearch],
  )

  const { data, isLoading, isError } = useQuery({
    queryKey: quotesKeys.list(params),
    queryFn: () => fetchQuotes(api, params),
  })

  const columns = useMemo(
    () => [
      columnHelper.accessor('number', {
        header: t('fields.number'),
        meta: { width: 90 },
        cell: (info) => (
          <Text size="sm" c="steel.7" fw={500}>
            {info.getValue()}
          </Text>
        ),
      }),
      columnHelper.accessor('status', {
        id: 'status',
        header: t('fields.status'),
        enableSorting: true,
        meta: { width: 120 },
        cell: (info) => {
          const id = info.getValue()
          return <StatusTag tone={status.metaFor(id).tone} label={status.labelFor(id)} />
        },
      }),
      columnHelper.accessor((row) => partyLabel(row), {
        id: 'party',
        header: t('fields.party'),
        enableSorting: false,
        meta: { width: 220 },
        cell: (info) => {
          const quote = info.row.original
          if (quote.customer_id) {
            return (
              <Text
                size="sm"
                component="span"
                style={{ cursor: 'pointer' }}
                onClick={(event) => {
                  event.stopPropagation()
                  navigate({ to: '/customers/$id', params: { id: quote.customer_id! } })
                }}
              >
                {info.getValue()}
              </Text>
            )
          }
          return (
            <Text size="sm" c="dimmed">
              {info.getValue()}
            </Text>
          )
        },
      }),
      columnHelper.accessor((row) => row.lines.length, {
        id: 'lines',
        header: t('fields.lines'),
        enableSorting: false,
        meta: { align: 'right', width: 80 },
      }),
      columnHelper.accessor((row) => formatMoney({ amount_minor: row.total_minor, currency: row.currency }, i18n.language), {
        id: 'total',
        header: t('fields.total'),
        enableSorting: false,
        meta: { align: 'right', width: 120 },
      }),
      columnHelper.accessor('valid_until', {
        id: 'valid_until',
        header: t('fields.validUntil'),
        enableSorting: false,
        meta: { width: 120 },
        cell: (info) => {
          const value = info.getValue()
          return (
            <Text size="sm" fw={300}>
              {value ? formatDate(value, i18n.language) : '—'}
            </Text>
          )
        },
      }),
      columnHelper.accessor('created_at', {
        header: t('fields.createdAt'),
        meta: { width: 160 },
        cell: (info) => (
          <Text size="sm" fw={300}>
            {formatDateTime(info.getValue(), i18n.language)}
          </Text>
        ),
      }),
    ],
    [t, i18n.language, status, navigate],
  )

  const filterCount = (customerId ? 1 : 0) + (statusFilter !== null ? 1 : 0)

  const clearFilters = () => {
    setCustomerId('')
    setStatusFilter(null)
    setPage(1)
  }

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
    enableSorting: !hasSearch,
    onSortingChange: handleSortingChange,
    getCoreRowModel: getCoreRowModel(),
  })

  return (
    <ListLayout
      title={t('list.title')}
      tabs={[{ label: t('list.tabAll'), count: data?.total ?? 0 }]}
      searchValue={search}
      onSearchChange={(value) => {
        setSearch(value)
        setPage(1)
      }}
      searchPlaceholder={t('list.searchPlaceholder')}
      filterCount={filterCount}
      onClearFilters={clearFilters}
      primaryAction={
        <Button leftSection={<IconPlus size={15} stroke={1.5} />} onClick={() => navigate({ to: '/quotes/new' })}>
          {t('list.new')}
        </Button>
      }
      pagination={<ListPagination page={page} pageSize={PAGE_SIZE} total={data?.total ?? 0} onChange={setPage} />}
      filters={
        <>
          <CustomerSelect
            label={t('list.filterCustomer')}
            required={false}
            placeholder={t('list.filterCustomer')}
            comboboxProps={{ withinPortal: false }}
            value={customerId}
            onChange={(next) => {
              setCustomerId(next)
              setPage(1)
            }}
          />
          <Select
            label={t('list.filterStatus')}
            placeholder={t('list.filterStatus')}
            clearable
            comboboxProps={{ withinPortal: false }}
            data={status.options}
            value={statusFilter}
            onChange={(value) => {
              setStatusFilter(value)
              setPage(1)
            }}
          />
        </>
      }
    >
      {isError && (
        <Text c="red" p="md">
          {t('list.loadError')}
        </Text>
      )}
      <Table
        highlightOnHover
        horizontalSpacing="md"
        stickyHeader
        stickyHeaderOffset={LIST_HEADER_HEIGHT}
        style={{ tableLayout: 'fixed', width: '100%' }}
      >
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
                    style={{
                      cursor: header.column.getCanSort() ? 'pointer' : undefined,
                      width: columnWidth(header.column),
                      whiteSpace: 'nowrap',
                    }}
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
              onClick={() => navigate({ to: '/quotes/$id', params: { id: row.original.id } })}
              style={{ cursor: 'pointer' }}
            >
              {row.getVisibleCells().map((cell) => (
                <Table.Td key={cell.id} ta={columnAlign(cell.column)} style={{ width: columnWidth(cell.column) }}>
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </Table.Td>
              ))}
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {!isLoading && data?.items.length === 0 && (
        <Text c="dimmed" p="md">
          {t('list.empty')}
        </Text>
      )}
    </ListLayout>
  )
}
