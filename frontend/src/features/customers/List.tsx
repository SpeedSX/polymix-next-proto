import { useMemo, useState } from 'react'
import { Button, Select, Table, Text, TextInput } from '@mantine/core'
import { IconPlus } from '@tabler/icons-react'
import { useDebouncedValue } from '@mantine/hooks'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper, flexRender, getCoreRowModel, useReactTable } from '@tanstack/react-table'
import type { RowData, SortingState, Updater } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import styles from './List.module.css'
import { LIST_HEADER_HEIGHT, ListLayout } from '../../components/ListLayout'
import { ListPagination } from '../../components/ListPagination'
import { StatusMark, renderStatusSelectOption, statusMetaFor } from '../../components/StatusBadge'
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

declare module '@tanstack/react-table' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    width?: number
  }
}

const STATUS_COLUMN_WIDTH = 56

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
      columnHelper.accessor('status', {
        id: 'status',
        header: '',
        enableSorting: false,
        meta: { width: STATUS_COLUMN_WIDTH },
        cell: (info) => {
          const statusId = info.getValue()
          const meta = statusDict.byId.get(statusId)
          return (
            <StatusMark
              statusKey={meta?.key}
              color={meta?.color}
              label={statusDict.labelFor(statusId)}
            />
          )
        },
      }),
      columnHelper.accessor('name', {
        header: t('fields.name'),
        cell: (info) => (
          <Text size="sm" fw={500} c="steel.8" component="span" className={styles.name}>
            {info.getValue()}
          </Text>
        ),
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

  const selectedStatus = statusMetaFor(statusDict.byId, statusFilter)
  const filterCount = (statusFilter ? 1 : 0) + (tagFilter.trim() !== '' ? 1 : 0)

  const clearFilters = () => {
    setStatusFilter(null)
    setTagFilter('')
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
    // The server ranks by BM25 score and ignores `sort` while a search
    // query is active — don't offer column sorting that would be a no-op.
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
      onExport={() => {}}
      primaryAction={
        <Button leftSection={<IconPlus size={15} stroke={1.5} />} onClick={() => navigate({ to: '/customers/new' })}>
          {t('list.new')}
        </Button>
      }
      pagination={<ListPagination page={page} pageSize={PAGE_SIZE} total={data?.total ?? 0} onChange={setPage} />}
      filters={
        <>
          <Select
            label={t('list.filterStatus')}
            placeholder={t('list.filterStatus')}
            // Render inline, not in a portal: a portalled option list lives
            // outside the filter Popover's DOM, so selecting an option reads as
            // a click-outside and closes the whole panel.
            comboboxProps={{ withinPortal: false }}
            data={statusDict.options}
            value={statusFilter}
            onChange={(value) => {
              setStatusFilter(value)
              setPage(1)
            }}
            clearable
            renderOption={renderStatusSelectOption(statusDict.byId)}
            leftSection={
              statusFilter != null ? (
                <StatusMark
                  statusKey={selectedStatus.key}
                  color={selectedStatus.color}
                  label={statusDict.labelFor(Number(statusFilter) as CustomerStatusId)}
                  size={18}
                  withTooltip={false}
                  variant="filled"
                />
              ) : undefined
            }
          />
          <TextInput
            label={t('list.filterTag')}
            placeholder={t('list.filterTag')}
            value={tagFilter}
            onChange={(event) => {
              setTagFilter(event.currentTarget.value)
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
      <Table highlightOnHover horizontalSpacing="md" stickyHeader stickyHeaderOffset={LIST_HEADER_HEIGHT}>
        <Table.Thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <Table.Tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => {
                const sortDirection = header.column.getIsSorted()
                const width = header.column.columnDef.meta?.width
                return (
                  <Table.Th
                    key={header.id}
                    onClick={header.column.getToggleSortingHandler()}
                    style={{
                      cursor: header.column.getCanSort() ? 'pointer' : undefined,
                      width,
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
              onClick={() => navigate({ to: '/customers/$id', params: { id: row.original.id } })}
              style={{ cursor: 'pointer' }}
            >
              {row.getVisibleCells().map((cell) => (
                <Table.Td key={cell.id} style={{ width: cell.column.columnDef.meta?.width }}>
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
