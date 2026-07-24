import { useState } from 'react'
import {
  Alert,
  Badge,
  Box,
  Button,
  Group,
  Loader,
  Menu,
  Modal,
  Paper,
  Stack,
  Text,
  Textarea,
} from '@mantine/core'
import {
  IconArrowRight,
  IconBox,
  IconCopy,
  IconDotsVertical,
  IconPlus,
  IconRefresh,
  IconSend,
  IconTrash,
} from '@tabler/icons-react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { PageHeader } from '../../components/PageHeader'
import { StatusTag } from '../../components/StatusBadge'
import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { useForm, zodResolver } from '@mantine/form'
import { formatDate, formatDateTime } from '../../lib/dates'
import { formatMoney } from '../../lib/money'
import { TextInput } from '@mantine/core'
import {
  cloneQuote,
  convertQuoteToOrder,
  deleteQuote,
  fetchQuote,
  quotesKeys,
  repriceQuote,
  setQuoteStatus,
  updateQuote,
} from './api'
import { LineRow } from './LineRow'
import { PartyInput } from './PartyInput'
import { useQuoteStatus } from './useQuoteStatus'
import {
  canEditQuote,
  headerToNewQuoteFields,
  lineToNewLine,
  QUOTE_STATUS,
  quoteHeaderFrom,
  quoteHeaderSchema,
} from './types'
import type { Adjustment, NewQuote, NewQuoteLine, Quote, QuoteHeaderValues, QuoteStatusId } from './types'

export function QuoteDetail() {
  const { t } = useTranslation('quotes')
  const { id } = useParams({ from: '/quotes/$id' })
  const api = useApi()

  const { data: quote, isLoading, isError } = useQuery({
    queryKey: quotesKeys.detail(id),
    queryFn: () => fetchQuote(api, id),
  })

  if (isLoading) {
    return <Loader />
  }
  if (isError || !quote) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }
  // Remount the editor per quote id so the header form re-seeds from server truth.
  return <QuoteEditor key={quote.id} quote={quote} />
}

function QuoteEditor({ quote }: { quote: Quote }) {
  const { t, i18n } = useTranslation('quotes')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const status = useQuoteStatus()
  const editable = canEditQuote(quote.status)

  const [actionError, setActionError] = useState<string | null>(null)
  const [changedLines, setChangedLines] = useState<Set<string>>(new Set())
  const [convertOpen, setConvertOpen] = useState(false)

  const form = useForm<QuoteHeaderValues>({
    initialValues: quoteHeaderFrom(quote),
    validate: zodResolver(quoteHeaderSchema),
  })

  const money = (minor: number) => formatMoney({ amount_minor: minor, currency: quote.currency }, i18n.language)

  const buildNewQuote = (lines: NewQuoteLine[]): NewQuote => ({
    ...headerToNewQuoteFields(form.values),
    lines,
  })

  const updateMutation = useMutation({
    mutationFn: (body: NewQuote) => updateQuote(api, quote.id, body),
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(quotesKeys.detail(quote.id), updated)
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'form.unexpectedError')),
  })

  const commitLines = (lines: NewQuoteLine[]) => updateMutation.mutate(buildNewQuote(lines))
  const currentLines = (): NewQuoteLine[] => quote.lines.map(lineToNewLine)
  const commitHeader = () => {
    if (!form.validate().hasErrors) {
      updateMutation.mutate(buildNewQuote(currentLines()))
    }
  }

  const statusMutation = useMutation({
    mutationFn: (next: QuoteStatusId) => setQuoteStatus(api, quote.id, next),
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(quotesKeys.detail(quote.id), updated)
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
    },
    onError: (err) => {
      if (err instanceof ApiError && err.code === 'quote_status_transition' && err.details) {
        setActionError(
          t('errors.status_transition', {
            from: status.labelFor(Number(err.details.from) as QuoteStatusId),
            to: status.labelFor(Number(err.details.to) as QuoteStatusId),
          }),
        )
      } else {
        setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
  })

  const repriceMutation = useMutation({
    mutationFn: () => repriceQuote(api, quote.id),
    onSuccess: (result) => {
      setActionError(null)
      setChangedLines(new Set(result.changed_line_ids))
      queryClient.setQueryData(quotesKeys.detail(quote.id), result.quote)
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'form.unexpectedError')),
  })

  const cloneMutation = useMutation({
    mutationFn: () => cloneQuote(api, quote.id),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
      void navigate({ to: '/quotes/$id', params: { id: created.id } })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'form.unexpectedError')),
  })

  const convertMutation = useMutation({
    mutationFn: () => convertQuoteToOrder(api, quote.id),
    onSuccess: (order) => {
      setConvertOpen(false)
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
      void navigate({ to: '/orders/$id', params: { id: order.id } })
    },
    onError: (err) => {
      setConvertOpen(false)
      setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
    },
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteQuote(api, quote.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
      void navigate({ to: '/quotes' })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'detail.deleteError')),
  })

  const busy = updateMutation.isPending

  // --- line operations -------------------------------------------------------
  const patchLine = (lineId: string, fn: (line: NewQuoteLine) => NewQuoteLine) =>
    commitLines(currentLines().map((line) => (line.line_id === lineId ? fn(line) : line)))

  const onQtyChange = (lineId: string, qty: number) => patchLine(lineId, (line) => ({ ...line, qty }))
  const onManualChange = (lineId: string, p: { description: string; qty: number; unitMinor: number }) =>
    patchLine(lineId, (line) =>
      line.kind === 'manual' ? { ...line, description: p.description, qty: p.qty, unit_minor: p.unitMinor } : line,
    )
  const onApplyAdjustment = (lineId: string, adjustment: Adjustment) =>
    patchLine(lineId, (line) => (line.kind === 'manual' ? line : { ...line, adjustment }))
  const onRemoveAdjustment = (lineId: string) =>
    patchLine(lineId, (line) => (line.kind === 'manual' ? line : { ...line, adjustment: null }))
  const onRemove = (lineId: string) => commitLines(currentLines().filter((line) => line.line_id !== lineId))
  const onDuplicate = (lineId: string) => {
    const lines = currentLines()
    const found = lines.find((line) => line.line_id === lineId)
    if (found) {
      commitLines([...lines, { ...found, line_id: undefined }])
    }
  }
  const addManualLine = () =>
    commitLines([...currentLines(), { kind: 'manual', description: '', qty: 1, unit_minor: 0 }])

  const engineTotal = quote.lines
    .filter((l) => l.kind !== 'manual')
    .reduce((sum, l) => sum + l.pricing.final_total_minor, 0)
  const manualTotal = quote.lines
    .filter((l) => l.kind === 'manual')
    .reduce((sum, l) => sum + l.qty * l.unit_minor, 0)
  const engineCount = quote.lines.filter((l) => l.kind !== 'manual').length
  const manualCount = quote.lines.filter((l) => l.kind === 'manual').length

  const meta = status.metaFor(quote.status)
  const showConvert = quote.status === QUOTE_STATUS.Accepted && !quote.order_id

  const actions = editable ? (
    <>
      <Button
        variant="subtle"
        leftSection={<IconRefresh size={15} stroke={1.5} />}
        loading={repriceMutation.isPending}
        onClick={() => repriceMutation.mutate()}
      >
        {t('detail.reprice')}
      </Button>
      <Button
        variant="subtle"
        leftSection={<IconCopy size={15} stroke={1.5} />}
        loading={cloneMutation.isPending}
        onClick={() => cloneMutation.mutate()}
      >
        {t('detail.clone')}
      </Button>
      <Button
        leftSection={<IconSend size={15} stroke={1.5} />}
        loading={statusMutation.isPending}
        onClick={() => statusMutation.mutate(QUOTE_STATUS.Sent)}
      >
        {t('detail.send')}
      </Button>
      <Menu position="bottom-end">
        <Menu.Target>
          <Button variant="subtle" color="gray" px="xs" aria-label={t('detail.more')}>
            <IconDotsVertical size={16} stroke={1.5} />
          </Button>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item
            color="red"
            leftSection={<IconTrash size={15} stroke={1.5} />}
            onClick={() => deleteMutation.mutate()}
          >
            {t('detail.deleteDraft')}
          </Menu.Item>
        </Menu.Dropdown>
      </Menu>
    </>
  ) : (
    <>
      {meta.allowedTargets.map((next) => (
        <Button
          key={next}
          variant="light"
          loading={statusMutation.isPending}
          onClick={() => statusMutation.mutate(next)}
        >
          {t('detail.markAs', { status: status.labelFor(next) })}
        </Button>
      ))}
      <Button
        variant="subtle"
        leftSection={<IconCopy size={15} stroke={1.5} />}
        loading={cloneMutation.isPending}
        onClick={() => cloneMutation.mutate()}
      >
        {t('detail.clone')}
      </Button>
      {showConvert && (
        <Button leftSection={<IconArrowRight size={15} stroke={1.5} />} onClick={() => setConvertOpen(true)}>
          {t('detail.convert')}
        </Button>
      )}
      {quote.order_id && (
        <Button
          variant="light"
          leftSection={<IconBox size={15} stroke={1.5} />}
          onClick={() => navigate({ to: '/orders/$id', params: { id: quote.order_id! } })}
        >
          {t('detail.viewOrder')}
        </Button>
      )}
    </>
  )

  return (
    <Stack>
      <PageHeader
        breadcrumb={[t('list.title')]}
        title={quote.number}
        status={<StatusTag tone={meta.tone} label={status.labelFor(quote.status)} />}
        actions={actions}
      />

      {actionError && <Alert color="red">{actionError}</Alert>}

      {quote.order_id && (
        <Alert
          color="steel"
          icon={<IconBox size={18} />}
          styles={{ root: { borderLeft: '3px solid var(--mantine-color-steel-6)' } }}
        >
          <Group justify="space-between">
            <Text fz="sm">{t('detail.convertedBanner', { order: quote.order_id })}</Text>
            <Button
              size="xs"
              variant="subtle"
              onClick={() => navigate({ to: '/orders/$id', params: { id: quote.order_id! } })}
            >
              {t('detail.viewOrder')}
            </Button>
          </Group>
        </Alert>
      )}

      {/* Party & metadata band */}
      <Paper withBorder p="md" bg="steel.0">
        <Group align="flex-start" justify="space-between">
          <Box style={{ flex: 1, maxWidth: 360 }}>
            {editable ? (
              <PartyInput form={form} onCommit={commitHeader} />
            ) : (
              <Stack gap={2}>
                <Text fz={11} tt="uppercase" c="dimmed">
                  {quote.prospect ? t('party.prospect') : t('party.customer')}
                </Text>
                <Text fw={500}>{quote.customer_name ?? quote.prospect?.name ?? quote.customer_id}</Text>
              </Stack>
            )}
          </Box>
          <Group gap="xl" align="flex-start">
            <Stack gap={2}>
              <Text fz={11} tt="uppercase" c="dimmed">
                {t('fields.currency')}
              </Text>
              <Badge variant="outline" color="gray" radius={0} tt="none" fw={400}>
                {quote.currency}
              </Badge>
            </Stack>
            <Stack gap={2}>
              <Text fz={11} tt="uppercase" c="dimmed">
                {t('fields.validUntil')}
              </Text>
              {editable ? (
                <TextInput
                  size="xs"
                  type="date"
                  value={form.values.validUntil}
                  onChange={(event) => form.setFieldValue('validUntil', event.currentTarget.value)}
                  onBlur={commitHeader}
                />
              ) : (
                <Text fz="sm">{quote.valid_until ? formatDate(quote.valid_until, i18n.language) : '—'}</Text>
              )}
            </Stack>
            {quote.pricelist_version != null && (
              <Stack gap={2}>
                <Text fz={11} tt="uppercase" c="dimmed">
                  {t('fields.pricelist')}
                </Text>
                <Badge variant="light" color="steel" radius={0} tt="none" fw={400}>
                  v{quote.pricelist_version}
                </Badge>
              </Stack>
            )}
          </Group>
        </Group>
        <Text fz="xs" c="dimmed" mt="sm">
          {t('detail.createdBy', { user: quote.created_by, date: formatDateTime(quote.created_at, i18n.language) })}
        </Text>
      </Paper>

      {/* Lines */}
      <Group justify="space-between">
        <Group gap="xs">
          <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
            {t('detail.lines')}
          </Text>
          <Text fz="xs" c="dimmed">
            {t('detail.lineCount', { count: quote.lines.length })}
          </Text>
        </Group>
        {editable && (
          <Group gap="xs">
            <Button size="xs" variant="light" disabled leftSection={<IconPlus size={14} stroke={1.5} />}>
              {t('detail.addProduct')}
              <Badge size="xs" ml={6} variant="outline" color="gray">
                {t('detail.soon')}
              </Badge>
            </Button>
            <Button
              size="xs"
              variant="light"
              leftSection={<IconPlus size={14} stroke={1.5} />}
              onClick={() => navigate({ to: '/quotes/$id/compose', params: { id: quote.id }, search: {} })}
            >
              {t('detail.addExpert')}
            </Button>
            <Button size="xs" variant="light" leftSection={<IconPlus size={14} stroke={1.5} />} onClick={addManualLine}>
              {t('detail.addManual')}
            </Button>
          </Group>
        )}
      </Group>

      {quote.lines.length === 0 ? (
        <Text c="dimmed" fz="sm" py="md">
          {t('detail.noLines')}
        </Text>
      ) : (
        <Stack gap="xs">
          {quote.lines.map((line, index) => (
            <LineRow
              key={line.line_id}
              line={line}
              index={index}
              currency={quote.currency}
              editable={editable}
              changed={changedLines.has(line.line_id)}
              busy={busy}
              onQtyChange={onQtyChange}
              onManualChange={onManualChange}
              onDuplicate={onDuplicate}
              onRemove={onRemove}
              onEditInComposer={(lineId) =>
                navigate({ to: '/quotes/$id/compose', params: { id: quote.id }, search: { lineId } })
              }
              onApplyAdjustment={onApplyAdjustment}
              onRemoveAdjustment={onRemoveAdjustment}
            />
          ))}
        </Stack>
      )}

      {/* Footer: notes + totals */}
      <Group align="flex-start" justify="space-between" mt="md">
        <Box style={{ flex: 1, maxWidth: 480 }}>
          {editable ? (
            <Textarea
              label={t('fields.notes')}
              autosize
              minRows={3}
              {...form.getInputProps('notes')}
              onBlur={(event) => {
                form.getInputProps('notes').onBlur?.(event)
                commitHeader()
              }}
            />
          ) : (
            quote.notes && (
              <Stack gap={2}>
                <Text fz={11} tt="uppercase" c="dimmed">
                  {t('fields.notes')}
                </Text>
                <Text fz="sm">{quote.notes}</Text>
              </Stack>
            )
          )}
        </Box>
        <Stack gap={6} w={320}>
          <Group justify="space-between">
            <Text fz="sm" c="dimmed">
              {t('detail.engineLines', { count: engineCount })}
            </Text>
            <Text fz="sm">{money(engineTotal)}</Text>
          </Group>
          <Group justify="space-between">
            <Text fz="sm" c="dimmed">
              {t('detail.manualLines', { count: manualCount })}
            </Text>
            <Text fz="sm">{money(manualTotal)}</Text>
          </Group>
          <Paper bg="steel.9" c="white" p="sm" radius="sm">
            <Group justify="space-between">
              <Text fz="sm" style={{ opacity: 0.85 }}>
                {t('detail.quoteTotal')}
              </Text>
              <Text fz={22} fw={700}>
                {money(quote.total_minor)}
              </Text>
            </Group>
          </Paper>
          <Text fz={11} c="dimmed">
            {t('detail.totalFootnote')}
          </Text>
        </Stack>
      </Group>

      <Modal opened={convertOpen} onClose={() => setConvertOpen(false)} title={t('convert.title')} centered>
        <Stack>
          <Text fz="sm">{t('convert.body', { total: money(quote.total_minor) })}</Text>
          <Group justify="flex-end">
            <Button variant="subtle" onClick={() => setConvertOpen(false)}>
              {t('form.cancel')}
            </Button>
            <Button loading={convertMutation.isPending} onClick={() => convertMutation.mutate()}>
              {t('convert.confirm')}
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Stack>
  )
}
