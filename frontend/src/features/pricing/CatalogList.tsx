import { useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { Badge, Box, Button, Group, Stack, Table, Tabs, Text, TextInput, Title } from '@mantine/core'
import { IconPlus, IconSearch } from '@tabler/icons-react'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useApi } from '../../lib/api'
import { fetchList, fetchVersion, pricingKeys } from './api'
import { PRICING_ENTITIES, fromMicro, fromMinor } from './types'
import type { CatalogDoc, PricingEntitySegment } from './types'

interface Column {
  header: string
  cell: (doc: CatalogDoc) => ReactNode
}

function money(micro: unknown): string {
  return fromMicro(Number(micro ?? 0))
}

function columnsFor(entity: PricingEntitySegment, t: (key: string) => string): Column[] {
  switch (entity) {
    case 'formats':
      return [
        { header: t('fields.name'), cell: (d) => String(d.name ?? '') },
        { header: t('fields.widthMm'), cell: (d) => (d.trim_mm as number[])?.[0] ?? '' },
        { header: t('fields.heightMm'), cell: (d) => (d.trim_mm as number[])?.[1] ?? '' },
      ]
    case 'materials':
      return [
        { header: t('fields.name'), cell: (d) => String(d.name ?? '') },
        { header: t('fields.kind'), cell: (d) => String(d.kind ?? '') },
        {
          header: t('material.pricingBasis'),
          cell: (d) => {
            const basis = (d.pricing as { basis?: string } | undefined)?.basis
            return basis ? <Badge variant="light" color="steel" tt="none">{basis}</Badge> : null
          },
        },
        {
          header: t('list.unitPrice'),
          cell: (d) => money((d.pricing as { price_micro?: number } | undefined)?.price_micro),
        },
        {
          header: t('material.printable'),
          cell: (d) =>
            d.printable ? (
              <Badge variant="light" color="teal" tt="none">{t('list.yes')}</Badge>
            ) : (
              <Text c="dimmed" component="span">{t('list.no')}</Text>
            ),
        },
      ]
    case 'machines':
      return [
        { header: t('fields.name'), cell: (d) => String(d.name ?? '') },
        {
          header: t('machine.technology'),
          cell: (d) => <Badge variant="light" color="steel" tt="none">{String(d.technology ?? '')}</Badge>,
        },
        {
          header: t('list.sheetSize'),
          cell: (d) => {
            const s = d.sheet_size_mm as number[] | undefined
            return s ? `${s[0]} × ${s[1]}` : ''
          },
        },
        { header: t('machine.duplex'), cell: (d) => (d.duplex ? t('list.yes') : t('list.no')) },
        { header: t('fields.grammage'), cell: (d) => String(d.max_grammage_gsm ?? '') },
      ]
    case 'operations':
      return [
        { header: t('fields.name'), cell: (d) => String(d.name ?? '') },
        { header: t('fields.unitBasis'), cell: (d) => String(d.unit_basis ?? '') },
        { header: t('fields.setupCost'), cell: (d) => money(d.setup_micro) },
        { header: t('fields.unitPrice'), cell: (d) => money(d.unit_price_micro) },
      ]
    case 'policies':
      return [
        { header: t('fields.currency'), cell: (d) => String(d.currency ?? '') },
        { header: t('list.bands'), cell: (d) => (d.margin_bands as unknown[] | undefined)?.length ?? 0 },
        {
          header: t('policy.roundingStep'),
          cell: (d) => fromMinor(Number((d.rounding as { step_minor?: number } | undefined)?.step_minor ?? 0)),
        },
        { header: t('policy.minPrice'), cell: (d) => fromMinor(Number(d.min_price_minor ?? 0)) },
      ]
  }
}

function searchText(entity: PricingEntitySegment, doc: CatalogDoc): string {
  const kind = entity === 'materials' ? String(doc.kind ?? '') : ''
  return `${String(doc.name ?? doc.currency ?? '')} ${kind}`.toLowerCase()
}

export function CatalogList() {
  const { t } = useTranslation('pricing')
  const navigate = useNavigate()
  const api = useApi()
  const [entity, setEntity] = useState<PricingEntitySegment>('formats')
  const [search, setSearch] = useState('')
  const columns = columnsFor(entity, t)

  const versionQuery = useQuery({ queryKey: pricingKeys.version(), queryFn: () => fetchVersion(api) })
  const { data, isLoading, isError } = useQuery({
    queryKey: pricingKeys.list(entity),
    queryFn: () => fetchList(api, entity),
  })

  const items = useMemo(() => {
    const all = data?.items ?? []
    const term = search.trim().toLowerCase()
    return term ? all.filter((doc) => searchText(entity, doc).includes(term)) : all
  }, [data, search, entity])

  return (
    <Stack gap="md">
      <Group justify="space-between" align="flex-end" wrap="wrap">
        <Box>
          <Text fz={11} fw={500} tt="uppercase" c="dimmed" style={{ letterSpacing: '0.08em' }}>
            {t('breadcrumb')}
          </Text>
          <Group gap="sm" align="center">
            <Title order={2} fz={24} lh={1.1}>
              {t('list.title')}
            </Title>
            {versionQuery.data && (
              <Badge variant="light" color="steel" tt="none">
                {t('list.version', { version: versionQuery.data.version })}
              </Badge>
            )}
          </Group>
        </Box>
        <Group gap="sm">
          <TextInput
            w={220}
            leftSection={<IconSearch size={15} stroke={1.5} />}
            placeholder={t('list.searchPlaceholder')}
            value={search}
            onChange={(event) => setSearch(event.currentTarget.value)}
          />
          <Button
            leftSection={<IconPlus size={15} stroke={1.5} />}
            onClick={() => navigate({ to: '/settings/catalog/$entity/new', params: { entity } })}
          >
            {t('list.new', { entity: t(`entitySingular.${entity}`) })}
          </Button>
        </Group>
      </Group>

      <Tabs
        value={entity}
        onChange={(value) => {
          setEntity((value as PricingEntitySegment) ?? 'formats')
          setSearch('')
        }}
      >
        <Tabs.List>
          {PRICING_ENTITIES.map((segment) => (
            <Tabs.Tab key={segment} value={segment}>
              {t(`entity.${segment}`)}
            </Tabs.Tab>
          ))}
        </Tabs.List>
      </Tabs>

      {isError ? (
        <Text c="red">{t('list.loadError')}</Text>
      ) : (
        <Table highlightOnHover horizontalSpacing="md" verticalSpacing="sm">
          <Table.Thead>
            <Table.Tr>
              {columns.map((column) => (
                <Table.Th key={column.header}>{column.header}</Table.Th>
              ))}
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {items.map((doc) => (
              <Table.Tr
                key={String(doc.id)}
                style={{ cursor: 'pointer' }}
                onClick={() =>
                  navigate({ to: '/settings/catalog/$entity/$id', params: { entity, id: String(doc.id) } })
                }
              >
                {columns.map((column) => (
                  <Table.Td key={column.header}>{column.cell(doc)}</Table.Td>
                ))}
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      {!isLoading && !isError && items.length === 0 && <Text c="dimmed">{t('list.empty')}</Text>}
    </Stack>
  )
}
