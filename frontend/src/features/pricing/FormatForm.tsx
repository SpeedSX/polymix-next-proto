import { NumberInput, SimpleGrid, Stack, TextInput } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { PricingFormShell } from './PricingForm'
import { usePricingForm } from './usePricingForm'
import type { EntityFormProps } from './PricingForm'
import { formatFormSchema, toFormatDoc } from './types'
import type { FormatFormValues } from './types'

export function FormatForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  breadcrumb,
  title,
  headerActions,
}: EntityFormProps & { initialValues: FormatFormValues }) {
  const { t } = useTranslation('pricing')
  const { form, submitting, formError, submit } = usePricingForm({
    schema: formatFormSchema,
    initialValues,
    toDoc: toFormatDoc,
    onSubmit,
    onSuccess,
  })

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
        <SimpleGrid cols={{ base: 1, sm: 2 }}>
          <NumberInput label={t('fields.widthMm')} min={1} withAsterisk {...form.getInputProps('width')} />
          <NumberInput label={t('fields.heightMm')} min={1} withAsterisk {...form.getInputProps('height')} />
        </SimpleGrid>
      </Stack>
    </PricingFormShell>
  )
}
