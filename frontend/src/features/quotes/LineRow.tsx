import { useState } from 'react'
import { ActionIcon, Badge, Box, Collapse, Group, NumberInput, Paper, Stack, Text, TextInput } from '@mantine/core'
import { IconChevronDown, IconChevronRight, IconCopy, IconPencil, IconRefresh, IconTrash } from '@tabler/icons-react'
import { useTranslation } from 'react-i18next'

import { formatMoney, fromMinorUnits, MONEY_DECIMAL_PATTERN, toMinorUnits } from '../../lib/money'
import { AdjustmentPanel } from './AdjustmentPanel'
import { Breakdown } from './Breakdown'
import { lineDescription, lineTotalMinor } from './types'
import type { Adjustment, ManualLine, QuoteLine } from './types'

const GRID = '34px 96px 1fr 78px 96px 120px 96px'

export interface LineRowProps {
  line: QuoteLine
  index: number
  currency: string
  editable: boolean
  changed: boolean
  busy: boolean
  onQtyChange: (lineId: string, qty: number) => void
  onManualChange: (lineId: string, patch: { description: string; qty: number; unitMinor: number }) => void
  onDuplicate: (lineId: string) => void
  onRemove: (lineId: string) => void
  onEditInComposer: (lineId: string) => void
  onApplyAdjustment: (lineId: string, adjustment: Adjustment) => void
  onRemoveAdjustment: (lineId: string) => void
}

export function LineRow(props: LineRowProps) {
  const { line, index, currency, editable, changed, busy } = props
  const { t, i18n } = useTranslation('quotes')
  const [open, setOpen] = useState(false)

  const money = (minor: number) => formatMoney({ amount_minor: minor, currency }, i18n.language)
  const isEngine = line.kind === 'template' || line.kind === 'spec'
  const total = lineTotalMinor(line)

  const adjustment = isEngine ? line.pricing.adjustment : null
  const engineTotal = isEngine ? line.pricing.engine_total_minor : total
  const qty = line.qty

  return (
    <Paper withBorder>
      <Box style={{ display: 'grid', gridTemplateColumns: GRID, alignItems: 'center', gap: 8, padding: '8px 12px' }}>
        <Text fz="xs" c="dimmed">
          {index + 1}
        </Text>
        <Badge variant="outline" color={line.kind === 'manual' ? 'gray' : 'steel'} radius={0} tt="none" fw={400}>
          {t(`lineType.${line.kind}`)}
        </Badge>

        {/* Description */}
        <Group gap={6} wrap="nowrap" style={{ minWidth: 0 }}>
          {isEngine && (
            <ActionIcon variant="subtle" size="sm" color="gray" onClick={() => setOpen((v) => !v)}>
              {open ? <IconChevronDown size={15} /> : <IconChevronRight size={15} />}
            </ActionIcon>
          )}
          {line.kind === 'manual' && editable ? (
            <ManualDescription {...props} />
          ) : (
            <Text fz="sm" fw={500} truncate>
              {lineDescription(line)}
            </Text>
          )}
          {changed && (
            <Group gap={2} wrap="nowrap" c="red">
              <IconRefresh size={13} />
              <Text fz={11}>{t('detail.repriceChanged')}</Text>
            </Group>
          )}
        </Group>

        {/* Qty */}
        {line.kind === 'manual' ? (
          <Box />
        ) : editable ? (
          <NumberInput
            size="xs"
            min={1}
            hideControls
            value={qty}
            disabled={busy}
            onChange={(v) => typeof v === 'number' && props.onQtyChange(line.line_id, v)}
          />
        ) : (
          <Text fz="sm" ta="right">
            {qty}
          </Text>
        )}

        {/* Unit */}
        <Text fz="sm" c="dimmed" ta="right">
          {line.kind === 'manual' ? '' : money(Math.round(engineTotal / qty))}
        </Text>

        {/* Line total */}
        <Box style={{ textAlign: 'right' }}>
          {adjustment && (
            <Text fz="xs" c="dimmed" td="line-through">
              {money(engineTotal)}
            </Text>
          )}
          <Text fz="sm" fw={600}>
            {money(total)}
          </Text>
        </Box>

        {/* Actions */}
        <Group gap={2} justify="flex-end" wrap="nowrap">
          {editable && line.kind !== 'manual' && (
            <ActionIcon
              variant="subtle"
              color="steel"
              aria-label={t('detail.editInComposer')}
              onClick={() => props.onEditInComposer(line.line_id)}
            >
              <IconPencil size={16} stroke={1.5} />
            </ActionIcon>
          )}
          {editable && line.kind === 'manual' && (
            <ActionIcon
              variant="subtle"
              color="gray"
              aria-label={t('detail.duplicate')}
              onClick={() => props.onDuplicate(line.line_id)}
            >
              <IconCopy size={16} stroke={1.5} />
            </ActionIcon>
          )}
          {editable && (
            <ActionIcon
              variant="subtle"
              color="red"
              aria-label={t('detail.removeLine')}
              disabled={busy}
              onClick={() => props.onRemove(line.line_id)}
            >
              <IconTrash size={16} stroke={1.5} />
            </ActionIcon>
          )}
        </Group>
      </Box>

      {isEngine && (
        <Collapse in={open}>
          <Box p="md" bg="steel.0" style={{ borderTop: '1px solid var(--mantine-color-gray-3)' }}>
            <Stack gap="md">
              <Stack gap="xs">
                <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
                  {t('detail.breakdownSnapshot')}
                </Text>
                <Breakdown
                  breakdown={line.pricing.breakdown}
                  currency={currency}
                  engineTotalMinor={line.pricing.engine_total_minor}
                  finalTotalMinor={line.pricing.final_total_minor}
                  adjustment={adjustment}
                />
              </Stack>
              {(editable || adjustment) && (
                <AdjustmentPanel
                  pricing={line.pricing}
                  currency={currency}
                  readOnly={!editable}
                  busy={busy}
                  onApply={(adj) => props.onApplyAdjustment(line.line_id, adj)}
                  onRemove={() => props.onRemoveAdjustment(line.line_id)}
                />
              )}
            </Stack>
          </Box>
        </Collapse>
      )}
    </Paper>
  )
}

/** Inline editable fields for a manual line (design 14a line 3). */
function ManualDescription({ line, currency, busy, onManualChange }: LineRowProps) {
  const { t, i18n } = useTranslation('quotes')
  const manual = line as ManualLine
  const [description, setDescription] = useState(manual.description)
  const [qty, setQty] = useState<number | ''>(manual.qty)
  const [unitPrice, setUnitPrice] = useState(fromMinorUnits(manual.unit_minor, currency, i18n.language))

  const commit = () => {
    if (qty === '' || !MONEY_DECIMAL_PATTERN.test(unitPrice)) {
      return
    }
    onManualChange(line.line_id, {
      description: description.trim(),
      qty,
      unitMinor: toMinorUnits(unitPrice, currency, i18n.language),
    })
  }

  return (
    <Group gap="xs" wrap="nowrap" style={{ flex: 1 }}>
      <TextInput
        size="xs"
        style={{ flex: 1 }}
        placeholder={t('fields.description')}
        value={description}
        disabled={busy}
        onChange={(event) => setDescription(event.currentTarget.value)}
        onBlur={commit}
      />
      <NumberInput
        size="xs"
        w={70}
        min={1}
        hideControls
        value={qty}
        disabled={busy}
        onChange={(v) => setQty(typeof v === 'number' ? v : '')}
        onBlur={commit}
      />
      <TextInput
        size="xs"
        w={90}
        placeholder={t('fields.unitPrice')}
        value={unitPrice}
        disabled={busy}
        onChange={(event) => setUnitPrice(event.currentTarget.value)}
        onBlur={commit}
      />
    </Group>
  )
}
