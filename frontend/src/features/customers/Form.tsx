import { useState } from 'react'
import type { ReactNode } from 'react'
import {
  Accordion,
  ActionIcon,
  Alert,
  Box,
  Button,
  Checkbox,
  Group,
  NumberInput,
  Radio,
  SegmentedControl,
  Select,
  SimpleGrid,
  Stack,
  Table,
  Textarea,
  TextInput,
  TagsInput,
} from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useMediaQuery } from '@mantine/hooks'
import { IconPlus, IconTrash } from '@tabler/icons-react'
import { useTranslation } from 'react-i18next'

import { PageHeader } from '../../components/PageHeader'
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
  breadcrumb: string[]
  title: ReactNode
  status?: ReactNode
  /** Rendered in a right-hand accent column beside the form fields. When
   * omitted the form is a single centered column (e.g. the create screen). */
  sidePanel?: ReactNode
}

export function CustomerForm({
  initialValues,
  onSubmit,
  onSuccess,
  onCancel,
  onConflict,
  breadcrumb,
  title,
  status,
  sidePanel,
}: CustomerFormProps) {
  const { t, i18n } = useTranslation('customers')
  const isNarrow = useMediaQuery('(max-width: 48em)')
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

  const fields = (
    <Stack>
      <Accordion multiple variant="separated" defaultValue={['general']}>
        <Accordion.Item value="general">
          <Accordion.Control>{t('sections.general')}</Accordion.Control>
          <Accordion.Panel>
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
              <SimpleGrid cols={{ base: 1, sm: 2 }}>
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
              </SimpleGrid>
              <TagsInput
                label={t('fields.tags')}
                value={form.values.tags}
                onChange={(value) => form.setFieldValue('tags', value)}
              />
            </Stack>
          </Accordion.Panel>
        </Accordion.Item>
      </Accordion>

      <Accordion multiple variant="separated" defaultValue={['contacts']}>
        <Accordion.Item value="contacts">
          <Accordion.Control>{`${t('sections.contacts')}: ${form.values.contacts.length}`}</Accordion.Control>
          <Accordion.Panel>
            <Stack>
              <Group justify="flex-end">
                <Button
                  variant="default"
                  size="xs"
                  leftSection={<IconPlus size={15} />}
                  onClick={() => form.insertListItem('contacts', { ...emptyContactFormValues })}
                >
                  {t('form.addContact')}
                </Button>
              </Group>
              {form.values.contacts.length > 0 && (
                <Table verticalSpacing="xs">
                  <Table.Thead>
                    <Table.Tr>
                      <Table.Th>{t('fields.contactName')}</Table.Th>
                      <Table.Th>{t('fields.contactRole')}</Table.Th>
                      <Table.Th>{t('fields.email')}</Table.Th>
                      <Table.Th>{t('fields.phone')}</Table.Th>
                      <Table.Th ta="center">{t('fields.primary')}</Table.Th>
                      <Table.Th />
                    </Table.Tr>
                  </Table.Thead>
                  <Table.Tbody>
                    {form.values.contacts.map((contact, index) => (
                      <Table.Tr
                        key={index}
                        bg={contact.isPrimary ? 'var(--mantine-color-steel-0)' : undefined}
                      >
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
                          <Group justify="center">
                            <Radio
                              checked={contact.isPrimary}
                              onChange={() => {
                                form.values.contacts.forEach((_, otherIndex) => {
                                  form.setFieldValue(`contacts.${otherIndex}.isPrimary`, otherIndex === index)
                                })
                              }}
                            />
                          </Group>
                        </Table.Td>
                        <Table.Td>
                          <ActionIcon
                            color="steel"
                            variant="subtle"
                            aria-label={t('form.removeContact')}
                            onClick={() => form.removeListItem('contacts', index)}
                          >
                            <IconTrash size={16} />
                          </ActionIcon>
                        </Table.Td>
                      </Table.Tr>
                    ))}
                  </Table.Tbody>
                </Table>
              )}
            </Stack>
          </Accordion.Panel>
        </Accordion.Item>
      </Accordion>

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
            <SimpleGrid cols={{ base: 1, sm: 2 }}>
              <NumberInput
                label={t('fields.paymentTermsDays')}
                min={0}
                max={365}
                {...form.getInputProps('paymentTermsDays')}
              />
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
              <Checkbox
                label={t('fields.hasCreditLimit')}
                checked={form.values.hasCreditLimit}
                onChange={(event) => form.setFieldValue('hasCreditLimit', event.currentTarget.checked)}
                mt={{ sm: 28 }}
              />
              {form.values.hasCreditLimit && (
                <TextInput label={t('fields.creditLimitAmount')} {...form.getInputProps('creditLimitAmount')} />
              )}
              {form.values.hasCreditLimit && (
                <Select
                  label={t('fields.currency')}
                  data={[...currencyOptions]}
                  {...form.getInputProps('creditLimitCurrency')}
                />
              )}
              <TextInput label={t('fields.iban')} {...form.getInputProps('iban')} />
              <TextInput label={t('fields.bankName')} {...form.getInputProps('bankName')} />
            </SimpleGrid>
          </Accordion.Panel>
        </Accordion.Item>
      </Accordion>

      <Accordion multiple variant="separated" defaultValue={['notes']}>
        <Accordion.Item value="notes">
          <Accordion.Control>{t('fields.notes')}</Accordion.Control>
          <Accordion.Panel>
            <Textarea {...form.getInputProps('notes')} />
          </Accordion.Panel>
        </Accordion.Item>
      </Accordion>
    </Stack>
  )

  const alerts = conflict ? (
    <Alert
      color="yellow"
      title={t('errors.customer_modified_title')}
      styles={{ title: { color: 'var(--mantine-color-gray-9)' }, message: { color: 'var(--mantine-color-gray-8)' } }}
    >
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
  )

  return (
    <form onSubmit={handleSubmit}>
      <Stack gap="lg">
        <PageHeader
          sticky
          breadcrumb={breadcrumb}
          title={title}
          status={status}
          actions={
            <>
              <Button variant="subtle" onClick={onCancel} disabled={submitting}>
                {t('form.cancel')}
              </Button>
              <Button type="submit" loading={submitting}>
                {t('form.save')}
              </Button>
            </>
          }
        />
        {alerts}
        {sidePanel ? (
          <Box
            style={{
              display: 'grid',
              gridTemplateColumns: isNarrow ? '1fr' : 'minmax(0, 1fr) 320px',
              alignItems: 'stretch',
            }}
          >
            <Box
              pr={isNarrow ? undefined : 'xl'}
              pb={isNarrow ? 'md' : undefined}
              style={{
                borderRight: isNarrow ? undefined : '1px solid var(--mantine-color-gray-3)',
                borderBottom: isNarrow ? '1px solid var(--mantine-color-gray-3)' : undefined,
              }}
            >
              {fields}
            </Box>
            <Box component="aside" px="lg" py="md" style={{ background: 'var(--mantine-color-steel-0)' }}>
              {sidePanel}
            </Box>
          </Box>
        ) : (
          <Box maw={720}>{fields}</Box>
        )}
      </Stack>
    </form>
  )
}
