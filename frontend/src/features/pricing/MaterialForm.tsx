import { ActionIcon, Box, Button, Group, NumberInput, SegmentedControl, SimpleGrid, Stack, Switch, Text, TextInput } from '@mantine/core'
import { IconPlus, IconTrash } from '@tabler/icons-react'
import { useTranslation } from 'react-i18next'

import { MoneyMicroInput } from './MoneyMicroInput'
import { PricingFormShell } from './PricingForm'
import { usePricingForm } from './usePricingForm'
import type { EntityFormProps } from './PricingForm'
import { MATERIAL_BASES, materialFormSchema, toMaterialDoc } from './types'
import type { MaterialBasis, MaterialFormValues } from './types'

export function MaterialForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  breadcrumb,
  title,
  headerActions,
}: EntityFormProps & { initialValues: MaterialFormValues }) {
  const { t } = useTranslation('pricing')
  const { form, submitting, formError, submit } = usePricingForm({
    schema: materialFormSchema,
    initialValues,
    toDoc: toMaterialDoc,
    onSubmit,
    onSuccess,
  })

  const basis = form.values.basis

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
        <SimpleGrid cols={{ base: 1, sm: 2 }}>
          <TextInput label={t('fields.name')} withAsterisk {...form.getInputProps('name')} />
          <TextInput label={t('fields.kind')} withAsterisk {...form.getInputProps('kind')} />
        </SimpleGrid>

        <Stack gap="xs">
          <Text ff="heading" fw={600} fz={13} tt="uppercase" c="dimmed">
            {t('material.pricingBasis')}
          </Text>
          <SegmentedControl
            data={MATERIAL_BASES.map((value) => ({ value, label: value }))}
            value={basis}
            onChange={(value) => form.setFieldValue('basis', value as MaterialBasis)}
            style={{ alignSelf: 'flex-start' }}
          />
          <Box style={{ background: 'var(--mantine-primary-color-light)', padding: 16 }}>
            {basis === 'per_sheet' ? (
              <SimpleGrid cols={{ base: 1, sm: 3 }}>
                <NumberInput label={t('fields.sheetWidthMm')} min={1} {...form.getInputProps('sheetWidth')} />
                <NumberInput label={t('fields.sheetHeightMm')} min={1} {...form.getInputProps('sheetHeight')} />
                <MoneyMicroInput label={t('material.pricePerSheet')} {...form.getInputProps('price')} />
              </SimpleGrid>
            ) : (
              <MoneyMicroInput
                label={t(`material.pricePer.${basis}`)}
                maw={240}
                {...form.getInputProps('price')}
              />
            )}
          </Box>
        </Stack>

        <Stack gap="xs">
          <Switch
            label={t('material.printable')}
            checked={form.values.printable}
            onChange={(event) => form.setFieldValue('printable', event.currentTarget.checked)}
          />
          {form.values.printable && (
            <NumberInput label={t('fields.grammage')} min={1} maw={240} {...form.getInputProps('grammage')} />
          )}
        </Stack>

        <Stack gap="xs">
          <Group>
            <Text ff="heading" fw={600} fz={13} tt="uppercase" c="dimmed">
              {t('material.attributes')}
            </Text>
            <Text c="dimmed" fz={12}>
              {t('material.attributesHint')}
            </Text>
            <Button
              variant="default"
              size="xs"
              ml="auto"
              leftSection={<IconPlus size={15} />}
              onClick={() => form.insertListItem('attrs', { key: '', value: '' })}
            >
              {t('form.add')}
            </Button>
          </Group>
          {form.values.attrs.map((_, index) => (
            <Group key={index} align="flex-end" gap="sm" wrap="nowrap">
              <TextInput placeholder={t('material.attrKey')} style={{ flex: 1 }} {...form.getInputProps(`attrs.${index}.key`)} />
              <TextInput placeholder={t('material.attrValue')} style={{ flex: 1 }} {...form.getInputProps(`attrs.${index}.value`)} />
              <ActionIcon
                color="steel"
                variant="subtle"
                aria-label={t('form.remove')}
                onClick={() => form.removeListItem('attrs', index)}
              >
                <IconTrash size={16} />
              </ActionIcon>
            </Group>
          ))}
        </Stack>
      </Stack>
    </PricingFormShell>
  )
}
