import { Group, SegmentedControl, Stack, TextInput } from '@mantine/core'
import type { UseFormReturnType } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { CustomerSelect } from '../orders/CustomerSelect'
import type { QuoteHeaderValues } from './types'

export interface PartyInputProps {
  form: UseFormReturnType<QuoteHeaderValues>
  /** Persist edits (Detail band); omitted on the New form. */
  onCommit?: () => void
}

/** Bill-to picker: a Customer / Prospect toggle switching between the
 * searchable customer select and free-text prospect fields (design 14a party
 * band). */
export function PartyInput({ form, onCommit }: PartyInputProps) {
  const { t } = useTranslation('quotes')
  const mode = form.values.partyMode

  return (
    <Stack gap="xs">
      <SegmentedControl
        size="xs"
        data={[
          { value: 'customer', label: t('party.customer') },
          { value: 'prospect', label: t('party.prospect') },
        ]}
        value={mode}
        onChange={(value) => {
          form.setFieldValue('partyMode', value as QuoteHeaderValues['partyMode'])
          onCommit?.()
        }}
      />
      {mode === 'customer' ? (
        <CustomerSelect
          label={t('party.customerLabel')}
          value={form.values.customerId}
          onChange={(id) => {
            form.setFieldValue('customerId', id)
            onCommit?.()
          }}
          error={form.errors.customerId}
        />
      ) : (
        <Stack gap="xs">
          <TextInput
            label={t('party.prospectName')}
            withAsterisk
            {...form.getInputProps('prospectName')}
            onBlur={(event) => {
              form.getInputProps('prospectName').onBlur?.(event)
              onCommit?.()
            }}
          />
          <Group grow>
            <TextInput
              label={t('party.prospectEmail')}
              {...form.getInputProps('prospectEmail')}
              onBlur={(event) => {
                form.getInputProps('prospectEmail').onBlur?.(event)
                onCommit?.()
              }}
            />
            <TextInput
              label={t('party.prospectPhone')}
              {...form.getInputProps('prospectPhone')}
              onBlur={(event) => {
                form.getInputProps('prospectPhone').onBlur?.(event)
                onCommit?.()
              }}
            />
          </Group>
        </Stack>
      )}
    </Stack>
  )
}
