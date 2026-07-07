import { useState } from 'react'
import { ActionIcon, Alert, Button, Group, NumberInput, Stack, Table, Text, Textarea, TextInput } from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { ApiError } from '../../lib/api'
import { formatMoney, toMinorUnits } from '../../lib/money'
import { mapApiErrorField, orderFormSchema, toNewOrder } from './types'
import type { Order, OrderFormValues } from './types'

export interface OrderFormProps {
  initialValues: OrderFormValues
  onSubmit: (data: ReturnType<typeof toNewOrder>) => Promise<Order>
  onSuccess: (order: Order) => void
  onCancel: () => void
}

export function OrderForm({ initialValues, onSubmit, onSuccess, onCancel }: OrderFormProps) {
  const { t } = useTranslation('orders')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const form = useForm<OrderFormValues>({
    initialValues,
    validate: zodResolver(orderFormSchema),
  })

  const currency = form.values.currency.toUpperCase()
  const total = form.values.lineItems.reduce(
    (sum, item) => sum + toMinorUnits(item.unitPrice, currency) * (item.quantity || 0),
    0,
  )

  const handleSubmit = form.onSubmit(async (values) => {
    setFormError(null)
    setSubmitting(true)
    try {
      const order = await onSubmit(toNewOrder(values))
      onSuccess(order)
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
      <Stack maw={720}>
        {formError && <Alert color="red">{formError}</Alert>}
        <Group grow>
          <TextInput label={t('fields.customerId')} withAsterisk {...form.getInputProps('customerId')} />
          <TextInput label={t('fields.currency')} withAsterisk maxLength={3} {...form.getInputProps('currency')} />
        </Group>

        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>{t('fields.description')}</Table.Th>
              <Table.Th>{t('fields.quantity')}</Table.Th>
              <Table.Th>{t('fields.unitPrice')}</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {form.values.lineItems.map((_, index) => (
              <Table.Tr key={index}>
                <Table.Td>
                  <TextInput {...form.getInputProps(`lineItems.${index}.description`)} />
                </Table.Td>
                <Table.Td>
                  <NumberInput min={1} {...form.getInputProps(`lineItems.${index}.quantity`)} />
                </Table.Td>
                <Table.Td>
                  <TextInput {...form.getInputProps(`lineItems.${index}.unitPrice`)} />
                </Table.Td>
                <Table.Td>
                  <ActionIcon
                    color="red"
                    variant="subtle"
                    disabled={form.values.lineItems.length <= 1}
                    onClick={() => form.removeListItem('lineItems', index)}
                  >
                    ✕
                  </ActionIcon>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
        <Group justify="space-between">
          <Button
            variant="subtle"
            onClick={() => form.insertListItem('lineItems', { description: '', quantity: 1, unitPrice: '' })}
          >
            {t('form.addLine')}
          </Button>
          <Text fw={600}>{t('fields.total')}: {formatMoney({ amount_minor: total, currency })}</Text>
        </Group>

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
