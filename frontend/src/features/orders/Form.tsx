import { useState } from 'react'
import { ActionIcon, Alert, Button, Group, NumberInput, Select, Stack, Table, Text, Textarea, TextInput } from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, validationMessage } from '../../lib/api'
import { formatMoney, toMinorUnits } from '../../lib/money'
import { CustomerSelect } from './CustomerSelect'
import { CURRENCY_OPTIONS, mapApiErrorField, orderFormSchema, toNewOrder } from './types'
import type { Order, OrderFormValues } from './types'

export interface OrderFormProps {
  initialValues: OrderFormValues
  onSubmit: (data: ReturnType<typeof toNewOrder>) => Promise<Order>
  onSuccess: (order: Order) => void
  onCancel: () => void
}

export function OrderForm({ initialValues, onSubmit, onSuccess, onCancel }: OrderFormProps) {
  const { t, i18n } = useTranslation('orders')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const form = useForm<OrderFormValues>({
    initialValues,
    validate: zodResolver(orderFormSchema),
  })

  const currency = form.values.currency.toUpperCase()
  const currencyOptions = CURRENCY_OPTIONS.includes(currency as (typeof CURRENCY_OPTIONS)[number])
    ? CURRENCY_OPTIONS
    : [...CURRENCY_OPTIONS, currency]
  const total = form.values.lineItems.reduce((sum, item) => {
    const unitPriceMinor = toMinorUnits(item.unitPrice, currency, i18n.language)
    return sum + (Number.isFinite(unitPriceMinor) ? unitPriceMinor : 0) * (item.quantity || 0)
  }, 0)

  const handleSubmit = form.onSubmit(async (values) => {
    setFormError(null)
    setSubmitting(true)
    try {
      const order = await onSubmit(toNewOrder(values, i18n.language))
      onSuccess(order)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'validation_failed' && err.details) {
        const unmatched: string[] = []
        for (const [field, fieldError] of Object.entries(err.details)) {
          const mappedField = mapApiErrorField(field)
          const message = validationMessage(fieldError, t)
          if (mappedField) {
            form.setFieldError(mappedField, message)
          } else {
            unmatched.push(message)
          }
        }
        if (unmatched.length > 0) {
          setFormError(unmatched.join(' '))
        }
      } else {
        setFormError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    } finally {
      setSubmitting(false)
    }
  })

  return (
    <form onSubmit={handleSubmit}>
      <Stack maw={720}>
        {formError && <Alert color="red">{formError}</Alert>}
        <Group grow align="flex-start">
          <CustomerSelect {...form.getInputProps('customerId')} />
          <Select
            label={t('fields.currency')}
            withAsterisk
            data={[...currencyOptions]}
            {...form.getInputProps('currency')}
          />
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
                <Table.Td style={{ verticalAlign: 'top' }}>
                  <TextInput {...form.getInputProps(`lineItems.${index}.description`)} />
                </Table.Td>
                <Table.Td style={{ verticalAlign: 'top' }}>
                  <NumberInput min={1} {...form.getInputProps(`lineItems.${index}.quantity`)} />
                </Table.Td>
                <Table.Td style={{ verticalAlign: 'top' }}>
                  <TextInput {...form.getInputProps(`lineItems.${index}.unitPrice`)} />
                </Table.Td>
                <Table.Td style={{ verticalAlign: 'top' }}>
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
          <Text fw={600}>{t('fields.total')}: {formatMoney({ amount_minor: total, currency }, i18n.language)}</Text>
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
