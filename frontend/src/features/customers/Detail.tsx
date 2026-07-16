import { useState, type ReactNode } from 'react'
import { Alert, Anchor, Badge, Button, Group, Loader, Stack, Table, Text, Title } from '@mantine/core'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { formatMoney } from '../../lib/money'
import { customersKeys, deleteCustomer, fetchCustomer, setCustomerStatus, updateCustomer } from './api'
import { CustomerForm } from './Form'
import { CUSTOMER_KIND, fromCustomer } from './types'
import type { Address, Customer, CustomerKindId, CustomerStatusId, NewCustomer } from './types'
import { useCustomerStatusDictionary } from './useCustomerStatusDictionary'

const KIND_LABEL_KEYS: Record<CustomerKindId, string> = {
  [CUSTOMER_KIND.LegalEntity]: 'kind.legalEntity',
  [CUSTOMER_KIND.Fop]: 'kind.fop',
  [CUSTOMER_KIND.Individual]: 'kind.individual',
}

function formatAddress(address: Address): string {
  return [address.street, [address.zip, address.city].filter(Boolean).join(' '), address.country]
    .filter(Boolean)
    .join(', ')
}

function websiteHref(url: string): string {
  return /^https?:\/\//i.test(url) ? url : `https://${url}`
}

function Field({ label, value }: { label: string; value: ReactNode }) {
  return (
    <Text>
      <Text span c="dimmed" size="sm">
        {label}:
      </Text>{' '}
      <Text span>{value}</Text>
    </Text>
  )
}

export function CustomerDetail() {
  const { t, i18n } = useTranslation('customers')
  const { id } = useParams({ from: '/customers/$id' })
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const statusDict = useCustomerStatusDictionary()
  const [editing, setEditing] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)

  const {
    data: customer,
    isLoading,
    isError,
  } = useQuery({
    queryKey: customersKeys.detail(id),
    queryFn: () => fetchCustomer(api, id),
  })

  const statusMutation = useMutation({
    mutationFn: (status: CustomerStatusId) => setCustomerStatus(api, id, status),
    onMutate: async (status) => {
      await queryClient.cancelQueries({ queryKey: customersKeys.detail(id) })
      const previous = queryClient.getQueryData<Customer>(customersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Customer>(customersKeys.detail(id), { ...previous, status })
      }
      return { previous }
    },
    onSuccess: (updated) => {
      setActionError(null)
      queryClient.setQueryData(customersKeys.detail(id), updated)
    },
    onError: (err, _status, context) => {
      if (context?.previous) {
        queryClient.setQueryData(customersKeys.detail(id), context.previous)
      }
      if (err instanceof ApiError && err.code === 'customer_status_transition' && err.details) {
        const from = Number(err.details.from) as CustomerStatusId
        const to = Number(err.details.to) as CustomerStatusId
        setActionError(
          t('errors.customer_status_transition', {
            from: statusDict.labelFor(from),
            to: statusDict.labelFor(to),
          }),
        )
      } else {
        setActionError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: customersKeys.all }),
  })

  const updateMutation = useMutation({
    mutationFn: (data: NewCustomer) => updateCustomer(api, id, data),
    onMutate: async (data) => {
      await queryClient.cancelQueries({ queryKey: customersKeys.detail(id) })
      const previous = queryClient.getQueryData<Customer>(customersKeys.detail(id))
      if (previous) {
        queryClient.setQueryData<Customer>(customersKeys.detail(id), { ...previous, ...data })
      }
      return { previous }
    },
    onSuccess: (updated) => queryClient.setQueryData(customersKeys.detail(id), updated),
    onError: (_err, _data, context) => {
      if (context?.previous) {
        queryClient.setQueryData(customersKeys.detail(id), context.previous)
      }
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: customersKeys.all }),
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteCustomer(api, id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: customersKeys.all })
      void navigate({ to: '/customers' })
    },
    onError: (err) => setActionError(apiErrorMessage(err, t, 'detail.deleteError')),
  })

  if (isLoading) {
    return <Loader />
  }

  if (isError || !customer) {
    return <Alert color="red">{t('detail.loadError')}</Alert>
  }

  if (editing) {
    return (
      <Stack>
        <Title order={2}>{customer.name}</Title>
        <CustomerForm
          initialValues={fromCustomer(customer, i18n.language)}
          onSubmit={(data) => updateMutation.mutateAsync(data)}
          onSuccess={() => setEditing(false)}
          onCancel={() => setEditing(false)}
        />
      </Stack>
    )
  }

  const meta = statusDict.byId.get(customer.status)
  const nextStatuses = meta?.allowed_targets ?? []
  const hasFinance =
    customer.payment_terms_days > 0 ||
    customer.credit_limit !== null ||
    customer.default_discount_bp > 0 ||
    !!customer.iban ||
    !!customer.bank_name

  return (
    <Stack>
      <Group justify="space-between">
        <Title order={2}>{customer.name}</Title>
        <Badge color={meta?.color}>{statusDict.labelFor(customer.status)}</Badge>
      </Group>
      {actionError && <Alert color="red">{actionError}</Alert>}

      <Field label={t('fields.kind')} value={t(KIND_LABEL_KEYS[customer.kind])} />
      {customer.legal_name && (
        <Field
          label={
            customer.kind === CUSTOMER_KIND.Individual ? t('fields.fullName') : t('fields.legalName')
          }
          value={customer.legal_name}
        />
      )}
      {customer.edrpou && <Field label={t('fields.edrpou')} value={customer.edrpou} />}
      {customer.tax_id && <Field label={t('fields.taxId')} value={customer.tax_id} />}
      {customer.vat_ipn && <Field label={t('fields.vatIpn')} value={customer.vat_ipn} />}
      {customer.industry && <Field label={t('fields.industry')} value={customer.industry} />}
      {customer.source && <Field label={t('fields.source')} value={customer.source} />}
      {customer.website && (
        <Field
          label={t('fields.website')}
          value={
            <Anchor href={websiteHref(customer.website)} target="_blank" rel="noopener noreferrer">
              {customer.website}
            </Anchor>
          }
        />
      )}
      {customer.tags.length > 0 && (
        <Group gap="xs">
          {customer.tags.map((tag) => (
            <Badge key={tag} variant="light">
              {tag}
            </Badge>
          ))}
        </Group>
      )}

      {customer.contacts.length > 0 && (
        <Stack gap="xs">
          <Title order={4}>{t('sections.contacts')}</Title>
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>{t('fields.contactName')}</Table.Th>
                <Table.Th>{t('fields.contactRole')}</Table.Th>
                <Table.Th>{t('fields.email')}</Table.Th>
                <Table.Th>{t('fields.phone')}</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {customer.contacts.map((contact, index) => (
                <Table.Tr key={index}>
                  <Table.Td>
                    {contact.name}
                    {contact.is_primary ? ` (${t('fields.primary')})` : ''}
                  </Table.Td>
                  <Table.Td>{contact.role}</Table.Td>
                  <Table.Td>{contact.email}</Table.Td>
                  <Table.Td>{contact.phone}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Stack>
      )}

      {(customer.legal_address || customer.delivery_address) && (
        <Stack gap="xs">
          <Title order={4}>{t('sections.addresses')}</Title>
          {customer.legal_address && (
            <Field label={t('fields.legalAddress')} value={formatAddress(customer.legal_address)} />
          )}
          {customer.delivery_address && (
            <Field label={t('fields.deliveryAddress')} value={formatAddress(customer.delivery_address)} />
          )}
        </Stack>
      )}

      <Stack gap="xs">
        <Title order={4}>{t('sections.finance')}</Title>
        <Field label={t('fields.defaultCurrency')} value={customer.default_currency} />
        {hasFinance && (
          <>
            {customer.payment_terms_days > 0 && (
              <Field label={t('fields.paymentTermsDays')} value={String(customer.payment_terms_days)} />
            )}
            {customer.credit_limit && (
              <Field
                label={t('fields.creditLimitAmount')}
                value={formatMoney(customer.credit_limit, i18n.language)}
              />
            )}
            {customer.default_discount_bp > 0 && (
              <Field
                label={t('fields.defaultDiscountPercent')}
                value={String(customer.default_discount_bp / 100)}
              />
            )}
            {customer.iban && <Field label={t('fields.iban')} value={customer.iban} />}
            {customer.bank_name && <Field label={t('fields.bankName')} value={customer.bank_name} />}
          </>
        )}
      </Stack>

      {customer.notes && <Field label={t('fields.notes')} value={customer.notes} />}

      <Group>
        {nextStatuses.map((next) => (
          <Button
            key={next}
            variant="light"
            loading={statusMutation.isPending}
            onClick={() => statusMutation.mutate(next)}
          >
            {t('actions.transitionTo', { status: statusDict.labelFor(next) })}
          </Button>
        ))}
        <Button variant="subtle" onClick={() => setEditing(true)}>
          {t('form.edit')}
        </Button>
        <Button
          color="red"
          variant="subtle"
          loading={deleteMutation.isPending}
          onClick={() => deleteMutation.mutate()}
        >
          {t('detail.delete')}
        </Button>
      </Group>
    </Stack>
  )
}
