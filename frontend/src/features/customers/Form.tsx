import { useState } from 'react'
import {
  Accordion,
  ActionIcon,
  Alert,
  Button,
  Checkbox,
  Fieldset,
  Group,
  NumberInput,
  Radio,
  SegmentedControl,
  Select,
  Stack,
  Table,
  Textarea,
  TextInput,
  TagsInput,
} from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, validationMessage } from '../../lib/api'
import { CURRENCY_OPTIONS } from '../orders/types'
import { CUSTOMER_KIND, customerFormSchema, emptyContactFormValues, mapApiErrorField, toNewCustomer } from './types'
import type { AddressFormValues, Customer, CustomerFormValues, CustomerKindId } from './types'

function isAddressEmpty(address: AddressFormValues): boolean {
  return !address.street && !address.zip && !address.city && !address.country
}

export interface CustomerFormProps {
  initialValues: CustomerFormValues
  onSubmit: (data: ReturnType<typeof toNewCustomer>) => Promise<Customer>
  onSuccess: (customer: Customer) => void
  onCancel: () => void
  /** Called when the save is rejected because the record was modified
   * concurrently (409 customer_modified) — wire it to reload the latest. */
  onConflict?: () => void
}

export function CustomerForm({ initialValues, onSubmit, onSuccess, onCancel, onConflict }: CustomerFormProps) {
  const { t, i18n } = useTranslation('customers')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const [conflict, setConflict] = useState(false)
  const form = useForm<CustomerFormValues>({
    initialValues,
    validate: zodResolver(customerFormSchema),
  })

  const kind = form.values.kind
  const currency = form.values.defaultCurrency.toUpperCase()
  const currencyOptions = CURRENCY_OPTIONS.includes(currency as (typeof CURRENCY_OPTIONS)[number])
    ? CURRENCY_OPTIONS
    : [...CURRENCY_OPTIONS, currency]

  const handleSubmit = form.onSubmit(async (values) => {
    setFormError(null)
    setConflict(false)
    setSubmitting(true)
    try {
      const customer = await onSubmit(toNewCustomer(values, i18n.language))
      onSuccess(customer)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'validation_failed' && err.details) {
        for (const [field, fieldError] of Object.entries(err.details)) {
          form.setFieldError(mapApiErrorField(field), validationMessage(fieldError, t))
        }
      } else if (err instanceof ApiError && err.code === 'customer_modified') {
        setConflict(true)
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
        {conflict ? (
          <Alert color="yellow" title={t('errors.customer_modified_title')}>
            <Stack gap="xs" align="flex-start">
              {t('errors.customer_modified')}
              {onConflict && (
                <Button variant="light" size="xs" onClick={onConflict}>
                  {t('form.reload')}
                </Button>
              )}
            </Stack>
          </Alert>
        ) : (
          formError && <Alert color="red">{formError}</Alert>
        )}

        <Fieldset legend={t('sections.general')}>
          <Stack>
            <SegmentedControl
              data={[
                { value: String(CUSTOMER_KIND.LegalEntity), label: t('kind.legalEntity') },
                { value: String(CUSTOMER_KIND.Fop), label: t('kind.fop') },
                { value: String(CUSTOMER_KIND.Individual), label: t('kind.individual') },
              ]}
              value={String(form.values.kind)}
              onChange={(value) => form.setFieldValue('kind', Number(value) as CustomerKindId)}
            />
            <TextInput
              label={kind === CUSTOMER_KIND.Individual ? t('fields.nameIndividual') : t('fields.name')}
              withAsterisk
              {...form.getInputProps('name')}
            />
            <TextInput
              label={kind === CUSTOMER_KIND.Individual ? t('fields.fullName') : t('fields.legalName')}
              {...form.getInputProps('legalName')}
            />
            {kind === CUSTOMER_KIND.LegalEntity && (
              <TextInput label={t('fields.edrpou')} {...form.getInputProps('edrpou')} />
            )}
            {kind !== CUSTOMER_KIND.LegalEntity && (
              <TextInput label={t('fields.taxId')} {...form.getInputProps('taxId')} />
            )}
            <TextInput label={t('fields.vatIpn')} {...form.getInputProps('vatIpn')} />
            <TextInput label={t('fields.industry')} {...form.getInputProps('industry')} />
            <TextInput label={t('fields.source')} {...form.getInputProps('source')} />
            <TextInput label={t('fields.website')} {...form.getInputProps('website')} />
            <TagsInput
              label={t('fields.tags')}
              value={form.values.tags}
              onChange={(value) => form.setFieldValue('tags', value)}
            />
          </Stack>
        </Fieldset>

        <Fieldset legend={t('sections.contacts')}>
          <Stack>
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>{t('fields.contactName')}</Table.Th>
                  <Table.Th>{t('fields.contactRole')}</Table.Th>
                  <Table.Th>{t('fields.email')}</Table.Th>
                  <Table.Th>{t('fields.phone')}</Table.Th>
                  <Table.Th>{t('fields.primary')}</Table.Th>
                  <Table.Th />
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {form.values.contacts.map((contact, index) => (
                  <Table.Tr key={index}>
                    <Table.Td>
                      <TextInput {...form.getInputProps(`contacts.${index}.name`)} />
                    </Table.Td>
                    <Table.Td>
                      <TextInput {...form.getInputProps(`contacts.${index}.role`)} />
                    </Table.Td>
                    <Table.Td>
                      <TextInput {...form.getInputProps(`contacts.${index}.email`)} />
                    </Table.Td>
                    <Table.Td>
                      <TextInput {...form.getInputProps(`contacts.${index}.phone`)} />
                    </Table.Td>
                    <Table.Td>
                      <Radio
                        checked={contact.isPrimary}
                        onChange={() => {
                          form.values.contacts.forEach((_, otherIndex) => {
                            form.setFieldValue(`contacts.${otherIndex}.isPrimary`, otherIndex === index)
                          })
                        }}
                      />
                    </Table.Td>
                    <Table.Td>
                      <ActionIcon color="red" variant="subtle" onClick={() => form.removeListItem('contacts', index)}>
                        ✕
                      </ActionIcon>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
            <Button
              variant="subtle"
              onClick={() => form.insertListItem('contacts', { ...emptyContactFormValues })}
            >
              {t('form.addContact')}
            </Button>
          </Stack>
        </Fieldset>

        <Accordion
          multiple
          variant="separated"
          defaultValue={[
            ...(!isAddressEmpty(initialValues.legalAddress) ? ['legal'] : []),
            ...(!isAddressEmpty(initialValues.deliveryAddress) ? ['delivery'] : []),
          ]}
        >
          <Accordion.Item value="legal">
            <Accordion.Control>{t('fields.legalAddress')}</Accordion.Control>
            <Accordion.Panel>
              <Stack>
                <TextInput label={t('fields.street')} {...form.getInputProps('legalAddress.street')} />
                <Group grow>
                  <TextInput label={t('fields.zip')} {...form.getInputProps('legalAddress.zip')} />
                  <TextInput label={t('fields.city')} {...form.getInputProps('legalAddress.city')} />
                </Group>
                <TextInput label={t('fields.country')} maxLength={2} {...form.getInputProps('legalAddress.country')} />
              </Stack>
            </Accordion.Panel>
          </Accordion.Item>
          <Accordion.Item value="delivery">
            <Accordion.Control>{t('fields.deliveryAddress')}</Accordion.Control>
            <Accordion.Panel>
              <Stack>
                <TextInput label={t('fields.street')} {...form.getInputProps('deliveryAddress.street')} />
                <Group grow>
                  <TextInput label={t('fields.zip')} {...form.getInputProps('deliveryAddress.zip')} />
                  <TextInput label={t('fields.city')} {...form.getInputProps('deliveryAddress.city')} />
                </Group>
                <TextInput
                  label={t('fields.country')}
                  maxLength={2}
                  {...form.getInputProps('deliveryAddress.country')}
                />
              </Stack>
            </Accordion.Panel>
          </Accordion.Item>
        </Accordion>

        <Accordion
          multiple
          variant="separated"
          defaultValue={
            initialValues.paymentTermsDays > 0 ||
            initialValues.hasCreditLimit ||
            initialValues.defaultDiscountPercent > 0 ||
            !!initialValues.iban ||
            !!initialValues.bankName
              ? ['finance']
              : []
          }
        >
          <Accordion.Item value="finance">
            <Accordion.Control>{t('sections.finance')}</Accordion.Control>
            <Accordion.Panel>
              <Stack>
                <NumberInput
                  label={t('fields.paymentTermsDays')}
                  min={0}
                  max={365}
                  {...form.getInputProps('paymentTermsDays')}
                />
                <Checkbox
                  label={t('fields.hasCreditLimit')}
                  checked={form.values.hasCreditLimit}
                  onChange={(event) => form.setFieldValue('hasCreditLimit', event.currentTarget.checked)}
                />
                {form.values.hasCreditLimit && (
                  <Group grow>
                    <TextInput label={t('fields.creditLimitAmount')} {...form.getInputProps('creditLimitAmount')} />
                    <Select
                      label={t('fields.currency')}
                      data={[...currencyOptions]}
                      {...form.getInputProps('creditLimitCurrency')}
                    />
                  </Group>
                )}
                <Select
                  label={t('fields.defaultCurrency')}
                  data={[...currencyOptions]}
                  {...form.getInputProps('defaultCurrency')}
                />
                <NumberInput
                  label={t('fields.defaultDiscountPercent')}
                  min={0}
                  max={100}
                  decimalScale={2}
                  {...form.getInputProps('defaultDiscountPercent')}
                />
                <TextInput label={t('fields.iban')} {...form.getInputProps('iban')} />
                <TextInput label={t('fields.bankName')} {...form.getInputProps('bankName')} />
              </Stack>
            </Accordion.Panel>
          </Accordion.Item>
        </Accordion>

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
