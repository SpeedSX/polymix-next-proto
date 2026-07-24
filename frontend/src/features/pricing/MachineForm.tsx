import { Box, NumberInput, SegmentedControl, SimpleGrid, Stack, Switch, Text, TextInput } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { MoneyMicroInput } from './MoneyMicroInput'
import { PricingFormShell } from './PricingForm'
import { usePricingForm } from './usePricingForm'
import type { EntityFormProps } from './PricingForm'
import { MACHINE_TECHNOLOGIES, machineFormSchema, toMachineDoc } from './types'
import type { MachineFormValues, MachineTechnology } from './types'

export function MachineForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  breadcrumb,
  title,
  headerActions,
}: EntityFormProps & { initialValues: MachineFormValues }) {
  const { t } = useTranslation('pricing')
  const { form, submitting, formError, submit } = usePricingForm({
    schema: machineFormSchema,
    initialValues,
    toDoc: toMachineDoc,
    onSubmit,
    onSuccess,
  })

  const digital = form.values.technology === 'digital'

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
        <TextInput label={t('fields.name')} withAsterisk {...form.getInputProps('name')} />

        <Stack gap="xs">
          <Text ff="heading" fw={600} fz={13} tt="uppercase" c="dimmed">
            {t('machine.technology')}
          </Text>
          <SegmentedControl
            data={MACHINE_TECHNOLOGIES.map((value) => ({ value, label: value }))}
            value={form.values.technology}
            onChange={(value) => form.setFieldValue('technology', value as MachineTechnology)}
            style={{ alignSelf: 'flex-start' }}
          />
        </Stack>

        <SimpleGrid cols={{ base: 1, sm: 3 }}>
          <NumberInput label={t('fields.sheetWidthMm')} min={1} {...form.getInputProps('sheetWidth')} />
          <NumberInput label={t('fields.sheetHeightMm')} min={1} {...form.getInputProps('sheetHeight')} />
          <NumberInput label={t('fields.grammage')} min={1} {...form.getInputProps('maxGrammage')} />
        </SimpleGrid>
        <SimpleGrid cols={{ base: 1, sm: 3 }}>
          <MoneyMicroInput label={t('fields.setupCost')} {...form.getInputProps('setup')} />
          <NumberInput label={t('machine.wasteFixedSheets')} min={0} {...form.getInputProps('wasteFixedSheets')} />
          <NumberInput label={t('machine.wastePercent')} min={0} max={100} {...form.getInputProps('wastePercent')} />
        </SimpleGrid>
        <Switch
          label={t('machine.duplex')}
          checked={form.values.duplex}
          onChange={(event) => form.setFieldValue('duplex', event.currentTarget.checked)}
        />

        <Box style={{ background: 'var(--mantine-primary-color-light)', padding: 16 }}>
          <Text ff="heading" fw={600} fz={12} tt="uppercase" c="dimmed" mb="sm">
            {t('machine.costModel')}
          </Text>
          {digital ? (
            <SimpleGrid cols={{ base: 1, sm: 2 }}>
              <MoneyMicroInput label={t('machine.clickMono')} {...form.getInputProps('clickMono')} />
              <MoneyMicroInput label={t('machine.clickColor')} {...form.getInputProps('clickColor')} />
            </SimpleGrid>
          ) : (
            <SimpleGrid cols={{ base: 1, sm: 2 }}>
              <MoneyMicroInput label={t('machine.platePrice')} {...form.getInputProps('platePrice')} />
              <MoneyMicroInput label={t('machine.runPrice')} {...form.getInputProps('runPrice')} />
            </SimpleGrid>
          )}
        </Box>
      </Stack>
    </PricingFormShell>
  )
}
