import { useMemo, useState } from 'react'
import { Button, Group, Pagination, Stack, Table, Text, Title } from '@mantine/core'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { customersKeys, fetchCustomers } from './api'
import type { Customer } from './types'

const PAGE_SIZE = 25
const DEFAULT_SORT = '-created_at'

function sortParam(sorting: SortingState): string {
  if (sorting.length === 0) {
    return DEFAULT_SORT
  }
  const [{ id, desc }] = sorting
  return desc ? `-${id}` : id
}

function pad(value: number): string {
  return value.toString().padStart(2, '0')
}

function formatCreatedAt(value: string): string {
  const date = new Date(value)
  return `${pad(date.getDate())}-${pad(date.getMonth() + 1)}-${date.getFullYear()} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

const columnHelper = createColumnHelper<Customer>()

export function CustomerList() {
  const { t } = useTranslation('customers')
  const navigate = useNavigate()
  const api = useApi()
  const [page, setPage] = useState(1)
  const [sorting, setSorting] = useState<SortingState>([{ id: 'created_at', desc: true }])

  const params = useMemo(() => ({ page, limit: PAGE_SIZE, sort: sortParam(sorting) }), [page, sorting])

  const { data, isLoading, isError } = useQuery({
    queryKey: customersKeys.list(params),
    queryFn: () => fetchCustomers(api, params),
  })

  const columns = useMemo(
    () => [
      columnHelper.accessor('name', { header: t('fields.name') }),
      columnHelper.accessor((row) => row.contact_name ?? '', { id: 'contact_name', header: t('fields.contactName') }),
      columnHelper.accessor((row) => row.email ?? '', { id: 'email', header: t('fields.email') }),
      columnHelper.accessor((row) => row.address?.city ?? '', { id: 'city', header: t('fields.city'), enableSorting: false }),
      columnHelper.accessor('created_at', { header: t('fields.createdAt'), cell: (info) => formatCreatedAt(info.getValue()) }),
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
