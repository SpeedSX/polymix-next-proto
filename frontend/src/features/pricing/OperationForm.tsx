import { Select, SimpleGrid, Stack, TextInput } from '@mantine/core'
import { useTranslation } from 'react-i18next'

import { MoneyMicroInput } from './MoneyMicroInput'
import { PricingFormShell } from './PricingForm'
import { usePricingForm } from './usePricingForm'
import type { EntityFormProps } from './PricingForm'
import { UNIT_BASES, operationFormSchema, toOperationDoc } from './types'
import type { OperationFormValues } from './types'

export function OperationForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  breadcrumb,
  title,
  headerActions,
}: EntityFormProps & { initialValues: OperationFormValues }) {
  const { t } = useTranslation('pricing')
  const { form, submitting, formError, submit } = usePricingForm({
    schema: operationFormSchema,
    initialValues,
    toDoc: toOperationDoc,
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
        <SimpleGrid cols={{ base: 1, sm: 2 }}>
          <TextInput label={t('fields.name')} withAsterisk {...form.getInputProps('name')} />
          <Select
            label={t('fields.unitBasis')}
            data={UNIT_BASES.map((basis) => ({ value: basis, label: t(`unitBasis.${basis}`) }))}
            allowDeselect={false}
            {...form.getInputProps('unitBasis')}
          />
          <MoneyMicroInput label={t('fields.setupCost')} {...form.getInputProps('setup')} />
          <MoneyMicroInput label={t('fields.unitPrice')} {...form.getInputProps('unitPrice')} />
        </SimpleGrid>
      </Stack>
    </PricingFormShell>
  )
}
