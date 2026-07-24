import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ActionIcon,
  Alert,
  Badge,
  Box,
  Button,
  Chip,
  Group,
  Loader,
  NumberInput,
  Paper,
  SegmentedControl,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
} from '@mantine/core'
import { useForm } from '@mantine/form'
import { useDebouncedValue } from '@mantine/hooks'
import { IconArrowLeft, IconCheck, IconPlus, IconTrash } from '@tabler/icons-react'
import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams, useSearch } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { formatMoney } from '../../lib/money'
import { fetchList } from '../pricing/api'
import type { CatalogDoc } from '../pricing/types'
import { estimate, fetchQuote, quotesKeys, updateQuote } from './api'
import { lineToNewLine } from './types'
import type { JobSpec, NewQuoteLine, SpecLine } from './types'

const TECHNOLOGY_OPTIONS = ['any', 'digital', 'offset'] as const
type TechnologyChoice = (typeof TECHNOLOGY_OPTIONS)[number]

interface ComponentRow {
  role: string
  pages: number
  front: number
  back: number
  material: string
  machine: string
}

interface OperationRow {
  operation: string
  material: string
  unitsMultiplier: number
}

interface ComposerValues {
  format: string
  quantity: number
  technology: TechnologyChoice
  description: string
  components: ComponentRow[]
  operations: OperationRow[]
}

function emptyComponent(): ComponentRow {
  return { role: '', pages: 1, front: 4, back: 4, material: '', machine: '' }
}

function catalogName(doc: CatalogDoc): string {
  return String(doc.name ?? doc.id ?? '')
}

function buildJobSpec(values: ComposerValues, quantity: number): JobSpec | null {
  if (!values.format) {
    return null
  }
  const components = values.components
    .filter((c) => c.role.trim() !== '' && c.material !== '')
    .map((c) => ({
      role: c.role.trim(),
      pages: c.pages,
      colors: `${c.front}/${c.back}`,
      material: c.material,
      machine: c.machine || null,
    }))
  if (components.length === 0) {
    return null
  }
  const operations = values.operations
    .filter((o) => o.operation !== '')
    .map((o) => {
      const params: Record<string, unknown> = {}
      if (o.material) {
        params.material = o.material
      }
      if (o.unitsMultiplier && o.unitsMultiplier > 1) {
        params.units_multiplier = o.unitsMultiplier
      }
      return { operation: o.operation, params }
    })
  const technology_allow =
    values.technology === 'any' ? null : values.technology === 'digital' ? ['digital'] : ['offset']
  return { format: values.format, quantity, components, operations, technology_allow }
}

function specToValues(line: SpecLine): ComposerValues {
  const tech = line.job_spec.technology_allow
  const technology: TechnologyChoice =
    !tech || tech.length !== 1 ? 'any' : tech[0] === 'digital' ? 'digital' : 'offset'
  return {
    format: line.job_spec.format,
    quantity: line.qty,
    technology,
    description: line.description,
    components: line.job_spec.components.map((c) => {
      const [front, back] = c.colors.split('/').map((n) => Number(n))
      return { role: c.role, pages: c.pages, front, back, material: c.material, machine: c.machine ?? '' }
    }),
    operations: line.job_spec.operations.map((o) => ({
      operation: o.operation,
      material: typeof o.params?.material === 'string' ? (o.params.material as string) : '',
      unitsMultiplier: typeof o.params?.units_multiplier === 'number' ? (o.params.units_multiplier as number) : 1,
    })),
  }
}

export function QuoteComposer() {
  const { t, i18n } = useTranslation('quotes')
  const { id } = useParams({ from: '/quotes/$id/compose' })
  const { lineId } = useSearch({ from: '/quotes/$id/compose' })
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const [saveError, setSaveError] = useState<string | null>(null)

  const quoteQuery = useQuery({ queryKey: quotesKeys.detail(id), queryFn: () => fetchQuote(api, id) })

  const formats = useQuery({ queryKey: ['pricing', 'formats'], queryFn: () => fetchList(api, 'formats') })
  const materials = useQuery({ queryKey: ['pricing', 'materials'], queryFn: () => fetchList(api, 'materials') })
  const machines = useQuery({ queryKey: ['pricing', 'machines'], queryFn: () => fetchList(api, 'machines') })
  const operations = useQuery({ queryKey: ['pricing', 'operations'], queryFn: () => fetchList(api, 'operations') })

  const editingLine = useMemo<SpecLine | null>(() => {
    if (!lineId || !quoteQuery.data) {
      return null
    }
    const found = quoteQuery.data.lines.find((l) => l.line_id === lineId)
    return found && found.kind === 'spec' ? found : null
  }, [lineId, quoteQuery.data])

  const form = useForm<ComposerValues>({
    initialValues: {
      format: '',
      quantity: 500,
      technology: 'any',
      description: '',
      components: [emptyComponent()],
      operations: [],
    },
  })

  // Seed the form once the line being edited (if any) has loaded.
  const seededRef = useRef(false)
  useEffect(() => {
    if (!seededRef.current && editingLine) {
      form.setValues(specToValues(editingLine))
      seededRef.current = true
    }
  }, [editingLine, form])

  const [extraQuantities, setExtraQuantities] = useState<number[]>([])
  const [newQty, setNewQty] = useState<number | ''>('')
  const [selectedQty, setSelectedQty] = useState<number | null>(null)

  const quantities = useMemo(() => {
    const all = new Set<number>([form.values.quantity, ...extraQuantities])
    return Array.from(all)
      .filter((q) => q >= 1)
      .sort((a, b) => a - b)
  }, [form.values.quantity, extraQuantities])

  const jobSpec = buildJobSpec(form.values, form.values.quantity)
  const [debouncedSpec] = useDebouncedValue(jobSpec, 350)
  const specKey = debouncedSpec ? JSON.stringify({ debouncedSpec, quantities }) : null

  const estimateQuery = useQuery({
    queryKey: ['estimate', specKey],
    queryFn: () => estimate(api, { job_spec: debouncedSpec!, quantities }),
    enabled: debouncedSpec !== null && quantities.length > 0,
    placeholderData: keepPreviousData,
    retry: false,
  })

  const focusedQty = selectedQty ?? form.values.quantity
  const focusedResult =
    estimateQuery.data?.results.find((r) => r.qty === focusedQty) ??
    estimateQuery.data?.results.find((r) => r.qty === form.values.quantity)

  const engineError =
    estimateQuery.error instanceof ApiError
      ? (estimateQuery.error.details?.engine_code as string | undefined) ?? estimateQuery.error.code
      : estimateQuery.error
        ? 'unknown'
        : null

  const saveMutation = useMutation({
    mutationFn: async () => {
      const quote = quoteQuery.data!
      const spec = buildJobSpec(form.values, form.values.quantity)
      if (!spec) {
        throw new Error('incomplete')
      }
      const newLine: NewQuoteLine = {
        kind: 'spec',
        line_id: editingLine?.line_id,
        job_spec: spec,
        description: form.values.description.trim() || t('composer.defaultDescription'),
        qty: form.values.quantity,
        adjustment: editingLine?.pricing.adjustment ?? null,
      }
      const existing = quote.lines.map(lineToNewLine)
      const lines = editingLine
        ? existing.map((l) => (l.line_id === editingLine.line_id ? newLine : l))
        : [...existing, newLine]
      return updateQuote(api, id, {
        customer_id: quote.customer_id ?? null,
        prospect: quote.prospect ?? null,
        currency: quote.currency,
        valid_until: quote.valid_until ?? null,
        notes: quote.notes,
        lines,
      })
    },
    onSuccess: (quote) => {
      queryClient.setQueryData(quotesKeys.detail(id), quote)
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
      void navigate({ to: '/quotes/$id', params: { id } })
    },
    onError: (err) => setSaveError(apiErrorMessage(err, t, 'form.unexpectedError')),
  })

  if (quoteQuery.isLoading) {
    return <Loader />
  }
  if (quoteQuery.isError || !quoteQuery.data) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  const quote = quoteQuery.data
  const currency = quote.currency
  const money = (minor: number) => formatMoney({ amount_minor: minor, currency }, i18n.language)

  const formatOptions = (formats.data?.items ?? []).map((doc) => ({ value: String(doc.id), label: catalogName(doc) }))
  const materialDocs = materials.data?.items ?? []
  const materialBasisOf = (id: string): string =>
    (materialDocs.find((doc) => String(doc.id) === id)?.pricing as { basis?: string } | undefined)?.basis ?? ''
  // Only materials whose pricing basis matches where they're used are offered —
  // components need per_sheet stock, operations need the operation's own basis
  // (e.g. spiral-binding is per_cm). A mismatch is engine error E204.
  const materialOptionsFor = (allowedBasis: string) =>
    materialDocs
      .filter((doc) => ((doc.pricing as { basis?: string } | undefined)?.basis ?? '') === allowedBasis)
      .map((doc) => ({ value: String(doc.id), label: catalogName(doc) }))
  const componentMaterialOptions = materialOptionsFor('per_sheet')
  const machineOptions = [
    { value: '', label: t('composer.machineAuto') },
    ...(machines.data?.items ?? []).map((doc) => ({ value: String(doc.id), label: catalogName(doc) })),
  ]
  const operationCatalog = operations.data?.items ?? []
  const operationOptions = operationCatalog.map((doc) => ({ value: String(doc.id), label: catalogName(doc) }))
  const operationBasis = (opId: string): string =>
    String(operationCatalog.find((doc) => doc.id === opId)?.unit_basis ?? '')

  return (
    <Group align="stretch" gap={0} wrap="nowrap" style={{ minHeight: '100%' }}>
      {/* Left composer pane */}
      <Box style={{ flex: 1.5, minWidth: 0, overflowY: 'auto' }} p="md">
        <Group justify="space-between" align="flex-start" mb="md">
          <Stack gap={2}>
            <Text fz={11} fw={500} tt="uppercase" c="dimmed" style={{ letterSpacing: '0.08em' }}>
              {quote.number} / {editingLine ? t('composer.editLine') : t('composer.newLine')}
            </Text>
            <Group gap="sm">
              <Text fz={22} fw={600}>
                {t('composer.title')}
              </Text>
              <Badge variant="outline" color="gray" radius={0} tt="none" fw={400}>
                {t('composer.staffOnly')}
              </Badge>
            </Group>
          </Stack>
          <Group>
            <Button
              variant="subtle"
              leftSection={<IconArrowLeft size={15} stroke={1.5} />}
              onClick={() => navigate({ to: '/quotes/$id', params: { id } })}
            >
              {t('form.cancel')}
            </Button>
            <Button
              leftSection={<IconCheck size={15} stroke={1.5} />}
              loading={saveMutation.isPending}
              disabled={jobSpec === null}
              onClick={() => {
                setSaveError(null)
                saveMutation.mutate()
              }}
            >
              {t('composer.save')}
            </Button>
          </Group>
        </Group>

        {saveError && (
          <Alert color="red" mb="md">
            {saveError}
          </Alert>
        )}

        <Stack gap="lg">
          {/* Job header */}
          <Stack gap="xs">
            <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
              {t('composer.jobHeader')}
            </Text>
            <Group grow align="flex-start">
              <Select
                label={t('composer.format')}
                searchable
                data={formatOptions}
                {...form.getInputProps('format')}
              />
              <NumberInput label={t('composer.quantity')} min={1} {...form.getInputProps('quantity')} />
            </Group>
            <Group align="flex-end" grow>
              <Stack gap={4}>
                <Text fz="sm" fw={500}>
                  {t('composer.technology')}
                </Text>
                <SegmentedControl
                  data={TECHNOLOGY_OPTIONS.map((value) => ({ value, label: t(`composer.tech.${value}`) }))}
                  {...form.getInputProps('technology')}
                />
              </Stack>
              <TextInput label={t('composer.description')} {...form.getInputProps('description')} />
            </Group>
          </Stack>

          {/* Components */}
          <Stack gap="xs">
            <Group justify="space-between">
              <Group gap="xs">
                <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
                  {t('composer.components')}
                </Text>
                <Text fz="xs" c="dimmed">
                  {t('composer.componentsHint')}
                </Text>
              </Group>
              <Button
                size="xs"
                variant="light"
                leftSection={<IconPlus size={14} stroke={1.5} />}
                onClick={() => form.insertListItem('components', emptyComponent())}
              >
                {t('composer.addComponent')}
              </Button>
            </Group>
            {form.values.components.map((component, index) => {
              const unprinted = component.front === 0 && component.back === 0
              return (
                <Paper key={index} withBorder p="sm">
                  <Group align="flex-end" gap="sm" wrap="nowrap">
                    <TextInput
                      label={t('composer.role')}
                      style={{ flex: 1.3 }}
                      {...form.getInputProps(`components.${index}.role`)}
                    />
                    <NumberInput
                      label={t('composer.pages')}
                      min={1}
                      w={80}
                      {...form.getInputProps(`components.${index}.pages`)}
                    />
                    <Group gap={4} align="flex-end" wrap="nowrap">
                      <NumberInput
                        label={t('composer.colors')}
                        min={0}
                        max={8}
                        w={56}
                        {...form.getInputProps(`components.${index}.front`)}
                      />
                      <Text pb={8}>/</Text>
                      <NumberInput min={0} max={8} w={56} {...form.getInputProps(`components.${index}.back`)} />
                    </Group>
                    <ActionIcon
                      color="red"
                      variant="subtle"
                      mb={4}
                      disabled={form.values.components.length <= 1}
                      aria-label={t('composer.removeComponent')}
                      onClick={() => form.removeListItem('components', index)}
                    >
                      <IconTrash size={16} stroke={1.5} />
                    </ActionIcon>
                  </Group>
                  <Group grow mt="xs" align="flex-start">
                    <Select
                      label={t('composer.material')}
                      searchable
                      data={componentMaterialOptions}
                      {...form.getInputProps(`components.${index}.material`)}
                    />
                    {unprinted ? (
                      <Text fz="xs" c="dimmed" mt={28}>
                        {t('composer.unprinted')}
                      </Text>
                    ) : (
                      <Select
                        label={t('composer.machinePin')}
                        data={machineOptions}
                        {...form.getInputProps(`components.${index}.machine`)}
                      />
                    )}
                  </Group>
                </Paper>
              )
            })}
          </Stack>

          {/* Operations */}
          <Stack gap="xs">
            <Group justify="space-between">
              <Group gap="xs">
                <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
                  {t('composer.operations')}
                </Text>
                <Text fz="xs" c="dimmed">
                  {t('composer.optional')}
                </Text>
              </Group>
              <Button
                size="xs"
                variant="light"
                leftSection={<IconPlus size={14} stroke={1.5} />}
                onClick={() => form.insertListItem('operations', { operation: '', material: '', unitsMultiplier: 1 })}
              >
                {t('composer.addOperation')}
              </Button>
            </Group>
            {form.values.operations.map((operation, index) => {
              const basis = operationBasis(operation.operation)
              return (
              <Paper key={index} withBorder p="sm">
                <Group align="flex-end" gap="sm" wrap="nowrap">
                  <Select
                    label={t('composer.operation')}
                    style={{ flex: 1.4 }}
                    data={operationOptions}
                    value={operation.operation}
                    error={form.getInputProps(`operations.${index}.operation`).error}
                    onChange={(value) => {
                      form.setFieldValue(`operations.${index}.operation`, value ?? '')
                      if (operation.material && materialBasisOf(operation.material) !== operationBasis(value ?? '')) {
                        form.setFieldValue(`operations.${index}.material`, '')
                      }
                    }}
                  />
                  <Badge variant="outline" color="gray" radius={0} tt="none" fw={400} mb={8}>
                    {basis || '—'}
                  </Badge>
                  <Select
                    label={t('composer.operationMaterial')}
                    clearable
                    style={{ flex: 1 }}
                    disabled={basis === ''}
                    placeholder={basis === '' ? t('composer.operationMaterialPending') : undefined}
                    data={basis === '' ? [] : materialOptionsFor(basis)}
                    {...form.getInputProps(`operations.${index}.material`)}
                  />
                  <NumberInput
                    label={t('composer.unitsMultiplier')}
                    min={1}
                    w={90}
                    {...form.getInputProps(`operations.${index}.unitsMultiplier`)}
                  />
                  <ActionIcon
                    color="red"
                    variant="subtle"
                    mb={4}
                    aria-label={t('composer.removeOperation')}
                    onClick={() => form.removeListItem('operations', index)}
                  >
                    <IconTrash size={16} stroke={1.5} />
                  </ActionIcon>
                </Group>
              </Paper>
              )
            })}
          </Stack>
        </Stack>
      </Box>

      {/* Right live breakdown pane */}
      <Box
        style={{
          flex: 1,
          minWidth: 340,
          maxWidth: 520,
          borderLeft: '1px solid var(--mantine-color-gray-3)',
          background: 'var(--mantine-color-steel-0)',
          overflowY: 'auto',
          opacity: estimateQuery.isFetching ? 0.5 : 1,
          transition: 'opacity 120ms',
        }}
        p="md"
      >
        {jobSpec === null ? (
          <Text c="dimmed" fz="sm" ta="center" mt="xl">
            {t('composer.idle')}
          </Text>
        ) : engineError ? (
          <Alert color="red" title={t('composer.engineErrorTitle')}>
            {t(`engineError.${engineError}`, { defaultValue: t('composer.engineErrorGeneric', { code: engineError }) })}
          </Alert>
        ) : (
          <Stack gap="lg">
            {/* Price summary */}
            <Paper bg="steel.9" c="white" p="md" radius="sm">
              <Text fz={11} tt="uppercase" style={{ letterSpacing: '0.08em', opacity: 0.75 }}>
                {t('composer.estimateFor', { qty: focusedQty })}
              </Text>
              <Text fz={40} fw={700} lh={1.1}>
                {focusedResult ? money(focusedResult.total_minor) : '—'}
              </Text>
              <Text fz="sm" style={{ opacity: 0.85 }}>
                {focusedResult ? t('composer.perUnit', { unit: money(focusedResult.unit_minor) }) : ''}
              </Text>
            </Paper>

            {/* Quantity ladder */}
            <Stack gap="xs">
              <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
                {t('composer.ladder')}
              </Text>
              <Group gap="xs">
                <Chip.Group value={String(focusedQty)} onChange={(v) => setSelectedQty(Number(v))}>
                  {quantities.map((qty) => (
                    <Chip key={qty} value={String(qty)} size="xs" variant="outline">
                      {qty}
                    </Chip>
                  ))}
                </Chip.Group>
              </Group>
              <Group gap="xs">
                <NumberInput
                  size="xs"
                  min={1}
                  w={110}
                  placeholder={t('composer.addQtyPlaceholder')}
                  value={newQty}
                  onChange={(v) => setNewQty(typeof v === 'number' ? v : '')}
                />
                <Button
                  size="xs"
                  variant="light"
                  disabled={newQty === '' || newQty < 1}
                  onClick={() => {
                    if (typeof newQty === 'number') {
                      setExtraQuantities((prev) => Array.from(new Set([...prev, newQty])))
                      setNewQty('')
                    }
                  }}
                >
                  {t('composer.addQty')}
                </Button>
              </Group>
              <Table fz="xs" withRowBorders={false} verticalSpacing={2}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>{t('composer.qty')}</Table.Th>
                    <Table.Th ta="right">{t('composer.unit')}</Table.Th>
                    <Table.Th ta="right">{t('composer.total')}</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {(estimateQuery.data?.results ?? []).map((result) => (
                    <Table.Tr
                      key={result.qty}
                      style={{ cursor: 'pointer', fontWeight: result.qty === focusedQty ? 600 : 400 }}
                      onClick={() => setSelectedQty(result.qty)}
                    >
                      <Table.Td>{result.qty}</Table.Td>
                      <Table.Td ta="right">{money(result.unit_minor)}</Table.Td>
                      <Table.Td ta="right">{money(result.total_minor)}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Stack>

            {/* Component & operation costs for the focused qty */}
            {focusedResult && (
              <Stack gap="xs">
                <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
                  {t('composer.components')}
                </Text>
                <Table fz="xs" withRowBorders={false} verticalSpacing={2}>
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>{t('breakdown.component')}</Table.Th>
                      <Table.Th>{t('breakdown.machine')}</Table.Th>
                      <Table.Th ta="right">{t('breakdown.sheets')}</Table.Th>
                      <Table.Th ta="right">{t('breakdown.cost')}</Table.Th>
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {focusedResult.breakdown.components.map((c) => (
                      <Table.Tr key={c.role}>
                        <Table.Td>{c.role}</Table.Td>
                        <Table.Td c="dimmed">{c.machine_id ?? '—'}</Table.Td>
                        <Table.Td ta="right">{c.sheets}</Table.Td>
                        <Table.Td ta="right">{money(Math.round(c.cost_micro / 10_000))}</Table.Td>
                      </Table.Tr>
                    ))}
                    {focusedResult.breakdown.operations.map((o) => (
                      <Table.Tr key={o.operation}>
                        <Table.Td>{o.operation}</Table.Td>
                        <Table.Td c="dimmed">{t('breakdown.operation')}</Table.Td>
                        <Table.Td ta="right">—</Table.Td>
                        <Table.Td ta="right">{money(Math.round(o.cost_micro / 10_000))}</Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
                <Group justify="space-between" pt="xs" style={{ borderTop: '1px solid var(--mantine-color-gray-3)' }}>
                  <Text fz="xs" c="dimmed">
                    {t('breakdown.productionCost')}
                  </Text>
                  <Text fz="xs" c="dimmed">
                    {money(Math.round(focusedResult.breakdown.cost_micro / 10_000))}
                  </Text>
                </Group>
                <Group justify="space-between">
                  <Text fz="sm" fw={600}>
                    {t('composer.total')}
                  </Text>
                  <Text fz="sm" fw={700}>
                    {money(focusedResult.total_minor)}
                  </Text>
                </Group>
              </Stack>
            )}
          </Stack>
        )}
      </Box>
    </Group>
  )
}
