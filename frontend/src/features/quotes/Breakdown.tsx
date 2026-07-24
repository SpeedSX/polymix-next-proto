import { Group, Stack, Table, Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { formatMoney } from '../../lib/money'
import type { Adjustment, Breakdown as BreakdownData } from './types'

// The engine carries component/operation costs in micro-units (1e6 per unit);
// money display is in minor units (1e2). Line totals are already minor.
function microToMinor(micro: number): number {
  return Math.round(micro / 10_000)
}

export interface BreakdownProps {
  breakdown: BreakdownData
  currency: string
  engineTotalMinor: number
  finalTotalMinor: number
  adjustment?: Adjustment | null
}

/** Cost breakdown for one engine-priced line (design 14a inline expand /
 * composer live panel): per-component and per-operation costs, the engine
 * total, and — when a line adjustment applies — the adjusted final total. */
export function Breakdown({ breakdown, currency, engineTotalMinor, finalTotalMinor, adjustment }: BreakdownProps) {
  const { t, i18n } = useTranslation('quotes')
  const money = (minor: number) => formatMoney({ amount_minor: minor, currency }, i18n.language)

  return (
    <Stack gap="xs">
      <Table withRowBorders={false} verticalSpacing={2} fz="xs">
        <Table.Thead>
          <Table.Tr>
            <Table.Th>{t('breakdown.component')}</Table.Th>
            <Table.Th>{t('breakdown.machine')}</Table.Th>
            <Table.Th ta="right">{t('breakdown.sheets')}</Table.Th>
            <Table.Th ta="right">{t('breakdown.cost')}</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {breakdown.components.map((component) => (
            <Table.Tr key={component.role}>
              <Table.Td>{component.role}</Table.Td>
              <Table.Td c="dimmed">{component.machine_id ?? '—'}</Table.Td>
              <Table.Td ta="right">{component.sheets}</Table.Td>
              <Table.Td ta="right">{money(microToMinor(component.cost_micro))}</Table.Td>
            </Table.Tr>
          ))}
          {breakdown.operations.map((operation) => (
            <Table.Tr key={operation.operation}>
              <Table.Td>{operation.operation}</Table.Td>
              <Table.Td c="dimmed">{t('breakdown.operation')}</Table.Td>
              <Table.Td ta="right">—</Table.Td>
              <Table.Td ta="right">{money(microToMinor(operation.cost_micro))}</Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>

      <Stack gap={2}>
        <Group justify="space-between">
          <Text fz="xs" c="dimmed">
            {t('breakdown.productionCost')}
          </Text>
          <Text fz="xs" c="dimmed">
            {money(microToMinor(breakdown.cost_micro))}
          </Text>
        </Group>
        <Group justify="space-between">
          <Text fz="xs">{t('breakdown.enginePrice')}</Text>
          <Text fz="xs" td={adjustment ? 'line-through' : undefined} c={adjustment ? 'dimmed' : undefined}>
            {money(engineTotalMinor)}
          </Text>
        </Group>
        {adjustment && (
          <Group justify="space-between">
            <Text fz="xs" fw={500}>
              {t(`adjustment.kind.${adjustment.kind}`)}
              {adjustment.reason ? ` · ${adjustment.reason}` : ''}
            </Text>
            <Text fz="xs" fw={500}>
              {money(finalTotalMinor)}
            </Text>
          </Group>
        )}
      </Stack>
    </Stack>
  )
}
