import { ActionIcon, Box, Button, Group, NumberInput, Select, SimpleGrid, Stack, Text, TextInput } from '@mantine/core'
import { IconArrowDown, IconArrowUp, IconPlus, IconTrash } from '@tabler/icons-react'
import { useTranslation } from 'react-i18next'

import { MoneyMicroInput } from './MoneyMicroInput'
import { PricingFormShell } from './PricingForm'
import { usePricingForm } from './usePricingForm'
import type { EntityFormProps } from './PricingForm'
import { CURRENCY_OPTIONS, policyFormSchema, toPolicyDoc } from './types'
import type { PolicyFormValues } from './types'

export function PolicyForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  breadcrumb,
  title,
  headerActions,
}: EntityFormProps & { initialValues: PolicyFormValues }) {
  const { t } = useTranslation('pricing')
  const { form, submitting, formError, submit } = usePricingForm({
    schema: policyFormSchema,
    initialValues,
    toDoc: toPolicyDoc,
    onSubmit,
    onSuccess,
  })

  const bands = form.values.bands
  const bandsError = typeof form.errors.bands === 'string' ? form.errors.bands : null

  return (
    <PricingFormShell
      breadcrumb={breadcrumb}
      title={title}
      submitting={submitting}
      formError={formError}
      onCancel={onCancel}
      onSubmit={submit}
      headerActions={headerActions}
    >
      <Stack>
        <Select
          label={t('fields.currency')}
          data={[...CURRENCY_OPTIONS]}
          allowDeselect={false}
          maw={200}
          {...form.getInputProps('currency')}
        />

        <Stack gap="xs">
          <Group>
            <Text ff="heading" fw={600} fz={13} tt="uppercase" c="dimmed">
              {t('policy.marginBands')}
            </Text>
            <Text c="dimmed" fz={12}>
              {t('policy.marginBandsHint')}
            </Text>
            <Button
              variant="default"
              size="xs"
              ml="auto"
              leftSection={<IconPlus size={15} />}
              onClick={() =>
                form.insertListItem('bands', {
                  minQty: (bands.at(-1)?.minQty ?? 0) + 100,
                  multiplier: '1.5',
                })
              }
            >
              {t('policy.addBand')}
            </Button>
          </Group>

          {bandsError && <Text c="red" fz="sm">{bandsError}</Text>}

          {bands.map((_, index) => {
            const pinned = index === 0
            return (
              <Group key={index} align="flex-end" gap="sm" wrap="nowrap">
                <NumberInput
                  label={index === 0 ? t('policy.minQty') : undefined}
                  min={1}
                  disabled={pinned}
                  description={pinned ? t('policy.firstBandPinned') : undefined}
                  style={{ flex: 1 }}
                  {...form.getInputProps(`bands.${index}.minQty`)}
                />
                <TextInput
                  label={index === 0 ? t('policy.multiplier') : undefined}
                  style={{ flex: 1 }}
                  {...form.getInputProps(`bands.${index}.multiplier`)}
                />
                <ActionIcon
                  variant="subtle"
                  color="steel"
                  aria-label={t('policy.moveUp')}
                  disabled={index <= 1}
                  onClick={() => form.reorderListItem('bands', { from: index, to: index - 1 })}
                >
                  <IconArrowUp size={16} />
                </ActionIcon>
                <ActionIcon
                  variant="subtle"
                  color="steel"
                  aria-label={t('policy.moveDown')}
                  disabled={pinned || index >= bands.length - 1}
                  onClick={() => form.reorderListItem('bands', { from: index, to: index + 1 })}
                >
                  <IconArrowDown size={16} />
                </ActionIcon>
                <ActionIcon
                  variant="subtle"
                  color="steel"
                  aria-label={t('form.remove')}
                  disabled={pinned}
                  onClick={() => form.removeListItem('bands', index)}
                >
                  <IconTrash size={16} />
                </ActionIcon>
              </Group>
            )
          })}
        </Stack>

        <Box>
          <Text ff="heading" fw={600} fz={13} tt="uppercase" c="dimmed" mb="xs">
            {t('policy.roundingFloor')}
          </Text>
          <SimpleGrid cols={{ base: 1, sm: 2 }}>
            <MoneyMicroInput label={t('policy.roundingStep')} {...form.getInputProps('roundingStep')} />
            <MoneyMicroInput label={t('policy.minPrice')} {...form.getInputProps('minPrice')} />
          </SimpleGrid>
        </Box>
      </Stack>
    </PricingFormShell>
  )
}
