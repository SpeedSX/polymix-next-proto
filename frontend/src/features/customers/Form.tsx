import { useState } from 'react'
import { Alert, Button, Group, Stack, Textarea, TextInput } from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { ApiError } from '../../lib/api'
import { customerFormSchema, mapApiErrorField, toNewCustomer } from './types'
import type { Customer, CustomerFormValues } from './types'

export interface CustomerFormProps {
  initialValues: CustomerFormValues
  onSubmit: (data: ReturnType<typeof toNewCustomer>) => Promise<Customer>
  onSuccess: (customer: Customer) => void
  onCancel: () => void
}

export function CustomerForm({ initialValues, onSubmit, onSuccess, onCancel }: CustomerFormProps) {
  const { t } = useTranslation('customers')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const form = useForm<CustomerFormValues>({
    initialValues,
    validate: zodResolver(customerFormSchema),
  })

  const handleSubmit = form.onSubmit(async (values) => {
    setFormError(null)
    setSubmitting(true)
    try {
      const customer = await onSubmit(toNewCustomer(values))
      onSuccess(customer)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'validation_failed' && err.details) {
        for (const [field, message] of Object.entries(err.details)) {
          form.setFieldError(mapApiErrorField(field), message)
        }
      } else {
        setFormError(err instanceof ApiError ? err.message : t('form.unexpectedError'))
      }
    } finally {
      setSubmitting(false)
    }
  })

  return (
    <form onSubmit={handleSubmit}>
      <Stack maw={480}>
        {formError && <Alert color="red">{formError}</Alert>}
        <TextInput label={t('fields.name')} withAsterisk {...form.getInputProps('name')} />
        <TextInput label={t('fields.contactName')} {...form.getInputProps('contactName')} />
        <TextInput label={t('fields.email')} {...form.getInputProps('email')} />
        <TextInput label={t('fields.phone')} {...form.getInputProps('phone')} />
        <TextInput label={t('fields.street')} {...form.getInputProps('address.street')} />
        <Group grow>
          <TextInput label={t('fields.zip')} {...form.getInputProps('address.zip')} />
          <TextInput label={t('fields.city')} {...form.getInputProps('address.city')} />
        </Group>
        <TextInput label={t('fields.country')} maxLength={2} {...form.getInputProps('address.country')} />
        <Textarea label={t('fields.notes')} {...form.getInputProps('notes')} />
        <Group justify="flex-end">
          <Button variant="subtle" onClick={onCancel} disabled={submitting}>
            {t('form.cancel')}
          </Button>
          <Button type="submit" loading={submitting}>
            {t('form.save')}
          </Button>
        </Group>
      </Stack>
    </form>
  )
}
