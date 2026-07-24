import { useState } from 'react'
import { Button, Group, NumberInput, Paper, SegmentedControl, Stack, Text, TextInput } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { formatMoney, fromMinorUnits, MONEY_DECIMAL_PATTERN, toMinorUnits } from '../../lib/money'
import type { Adjustment, AdjustmentKind, EnginePricing } from './types'

const MODES: AdjustmentKind[] = ['margin_override', 'discount', 'price_override']

export interface AdjustmentPanelProps {
  pricing: EnginePricing
  currency: string
  readOnly?: boolean
  onApply: (adjustment: Adjustment) => void
  onRemove: () => void
  busy?: boolean
}

/** Commercial override for one engine-priced line (design 15a). At most one
 * adjustment per line; the three modes are mutually exclusive. Margin re-runs
 * the engine server-side (rounding + min price still apply); discount and price
 * override are computed live off the stored engine total. */
export function AdjustmentPanel({ pricing, currency, readOnly, onApply, onRemove, busy }: AdjustmentPanelProps) {
  const { t, i18n } = useTranslation('quotes')
  const existing = pricing.adjustment
  const money = (minor: number) => formatMoney({ amount_minor: minor, currency }, i18n.language)

  const [mode, setMode] = useState<AdjustmentKind>(existing?.kind ?? 'discount')
  const [multiplier, setMultiplier] = useState<number | ''>(
    existing?.kind === 'margin_override' ? (existing.multiplier_bp ?? 0) / 10_000 : 1.7,
  )
  const [percent, setPercent] = useState<number | ''>(
    existing?.kind === 'discount' ? (existing.percent_bp ?? 0) / 100 : 10,
  )
  const [override, setOverride] = useState<string>(
    existing?.kind === 'price_override' ? fromMinorUnits(existing.total_minor ?? 0, currency, i18n.language) : '',
  )
  const [reason, setReason] = useState<string>(existing?.reason ?? '')
  const [error, setError] = useState<string | null>(null)

  const engineTotal = pricing.engine_total_minor

  let liveFinal: number | null = null
  if (mode === 'discount' && percent !== '') {
    liveFinal = engineTotal - Math.floor((engineTotal * Math.round(percent * 100)) / 10_000)
  } else if (mode === 'price_override' && MONEY_DECIMAL_PATTERN.test(override)) {
    liveFinal = toMinorUnits(override, currency, i18n.language)
  }

  function validate(): Adjustment | null {
    if (mode === 'margin_override') {
      if (multiplier === '' || multiplier <= 0) {
        setError(t('adjustment.errors.margin'))
        return null
      }
      return { kind: 'margin_override', multiplier_bp: Math.round(multiplier * 10_000), reason: reason.trim() || null }
    }
    if (mode === 'discount') {
      if (percent === '' || percent < 0 || percent > 100) {
        setError(t('adjustment.errors.discount'))
        return null
      }
      return { kind: 'discount', percent_bp: Math.round(percent * 100), reason: reason.trim() || null }
    }
    if (!MONEY_DECIMAL_PATTERN.test(override)) {
      setError(t('adjustment.errors.priceOverride'))
      return null
    }
    return { kind: 'price_override', total_minor: toMinorUnits(override, currency, i18n.language), reason: reason.trim() || null }
  }

  return (
    <Stack gap="xs">
      <Group gap="xs">
        <Text fz={11} fw={600} tt="uppercase" c="steel.7" style={{ letterSpacing: '0.06em' }}>
          {t('adjustment.commercial')}
        </Text>
      </Group>

      <SegmentedControl
        size="xs"
        disabled={readOnly}
        data={MODES.map((value) => ({ value, label: t(`adjustment.kind.${value}`) }))}
        value={mode}
        onChange={(value) => {
          setMode(value as AdjustmentKind)
          setError(null)
        }}
      />

      <Group grow align="flex-start">
        <Stack gap={4}>
          {mode === 'margin_override' && (
            <NumberInput
              label={t('adjustment.multiplier')}
              prefix="× "
              decimalScale={4}
              min={0}
              step={0.05}
              disabled={readOnly}
              value={multiplier}
              onChange={(v) => setMultiplier(typeof v === 'number' ? v : '')}
              description={t('adjustment.marginHelp')}
            />
          )}
          {mode === 'discount' && (
            <NumberInput
              label={t('adjustment.discount')}
              suffix=" %"
              min={0}
              max={100}
              disabled={readOnly}
              value={percent}
              onChange={(v) => setPercent(typeof v === 'number' ? v : '')}
              description={t('adjustment.discountHelp', { engine: money(engineTotal) })}
            />
          )}
          {mode === 'price_override' && (
            <TextInput
              label={t('adjustment.priceOverride', { currency })}
              disabled={readOnly}
              value={override}
              onChange={(event) => setOverride(event.currentTarget.value)}
              description={t('adjustment.priceOverrideHelp')}
            />
          )}
          {error && (
            <Text c="red" fz="xs">
              {error}
            </Text>
          )}
        </Stack>

        <Paper withBorder p="sm">
          <Group justify="space-between">
            <Text fz="xs" c="dimmed">
              {t('breakdown.enginePrice')}
            </Text>
            <Text fz="xs" c="dimmed">
              {money(engineTotal)}
            </Text>
          </Group>
          <Group justify="space-between">
            <Text fz="sm" fw={600}>
              {t('adjustment.final')}
            </Text>
            <Text fz="sm" fw={700}>
              {mode === 'margin_override'
                ? t('adjustment.recompute')
                : liveFinal !== null
                  ? money(liveFinal)
                  : '—'}
            </Text>
          </Group>
        </Paper>
      </Group>

      <TextInput
        label={t('adjustment.reason')}
        placeholder={t('adjustment.reasonPlaceholder')}
        disabled={readOnly}
        value={reason}
        onChange={(event) => setReason(event.currentTarget.value)}
      />

      {!readOnly && (
        <Group justify="space-between">
          <Button
            variant="subtle"
            color="red"
            size="xs"
            disabled={!existing}
            loading={busy}
            onClick={onRemove}
          >
            {t('adjustment.remove')}
          </Button>
          <Button
            size="xs"
            loading={busy}
            onClick={() => {
              setError(null)
              const adjustment = validate()
              if (adjustment) {
                onApply(adjustment)
              }
            }}
          >
            {t('adjustment.apply')}
          </Button>
        </Group>
      )}
    </Stack>
  )
}
