import { useMemo, useState } from 'react'
import { Badge, Button, Group, Pagination, Select, Stack, Table, Text, TextInput, Title } from '@mantine/core'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { formatDateTime } from '../../lib/dates'
import { customersKeys, fetchCustomers } from './api'
import type { Customer, CustomerStatusId } from './types'
import { useCustomerStatusDictionary } from './useCustomerStatusDictionary'

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

const columnHelper = createColumnHelper<Customer>()

export function CustomerList() {
  const { t, i18n } = useTranslation('customers')
  const navigate = useNavigate()
  const api = useApi()
  const statusDict = useCustomerStatusDictionary()
  const [page, setPage] = useState(1)
  const [sorting, setSorting] = useState<SortingState>([{ id: 'created_at', desc: true }])
  const [search, setSearch] = useState('')
  const [debouncedSearch] = useDebouncedValue(search, SEARCH_DEBOUNCE_MS)
  const [statusFilter, setStatusFilter] = useState<string | null>(null)
  const [tagFilter, setTagFilter] = useState('')
  const [debouncedTag] = useDebouncedValue(tagFilter, SEARCH_DEBOUNCE_MS)
  const hasSearch = debouncedSearch.trim() !== ''

  const params = useMemo(
    () => ({
      page,
      limit: PAGE_SIZE,
      sort: sortParam(sorting),
      q: hasSearch ? debouncedSearch.trim() : undefined,
      status: statusFilter ? (Number(statusFilter) as CustomerStatusId) : undefined,
      tag: debouncedTag.trim() !== '' ? debouncedTag.trim() : undefined,
    }),
    [page, sorting, hasSearch, debouncedSearch, statusFilter, debouncedTag],
  )

  const { data, isLoading, isError } = useQuery({
    queryKey: customersKeys.list(params),
    queryFn: () => fetchCustomers(api, params),
  })

  const columns = useMemo(
    () => [
      columnHelper.accessor('number', { header: t('fields.number') }),
      columnHelper.accessor('name', { header: t('fields.name') }),
      columnHelper.accessor((row) => row.edrpou ?? row.tax_id ?? '', {
        id: 'edrpou',
        header: t('fields.edrpouOrTaxId'),
        enableSorting: false,
      }),
      columnHelper.accessor('status', {
        header: t('fields.status'),
        enableSorting: false,
        cell: (info) => {
          const meta = statusDict.byId.get(info.getValue())
          return <Badge color={meta?.color}>{statusDict.labelFor(info.getValue())}</Badge>
        },
      }),
      columnHelper.accessor((row) => row.tags.join(', '), { id: 'tags', header: t('fields.tags'), enableSorting: false }),
      columnHelper.accessor(
        (row) => (row.contacts.find((c) => c.is_primary) ?? row.contacts[0])?.name ?? '',
        { id: 'primary_contact', header: t('fields.contactName'), enableSorting: false },
      ),
      columnHelper.accessor('created_at', {
        header: t('fields.createdAt'),
        cell: (info) => formatDateTime(info.getValue(), i18n.language),
      }),
    ],
    [t, i18n.language, statusDict],
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
        <Button onClick={() => navigate({ to: '/customers/new' })}>{t('list.new')}</Button>
      </Group>
      <Group grow>
        <TextInput
          placeholder={t('list.searchPlaceholder')}
          value={search}
          onChange={(event) => {
            setSearch(event.currentTarget.value)
            setPage(1)
          }}
        />
        <Select
          placeholder={t('list.filterStatus')}
          data={statusDict.options}
          value={statusFilter}
          onChange={(value) => {
            setStatusFilter(value)
            setPage(1)
          }}
          clearable
        />
        <TextInput
          placeholder={t('list.filterTag')}
          value={tagFilter}
          onChange={(event) => {
            setTagFilter(event.currentTarget.value)
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
              onClick={() => navigate({ to: '/customers/$id', params: { id: row.original.id } })}
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
