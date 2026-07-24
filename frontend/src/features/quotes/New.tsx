import { useState } from 'react'
import { Alert, Button, Loader, Select, Stack } from '@mantine/core'
import { useForm, zodResolver } from '@mantine/form'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { PageHeader } from '../../components/PageHeader'
import { ApiError, apiErrorMessage, useApi } from '../../lib/api'
import { createQuote, quotesKeys } from './api'
import { PartyInput } from './PartyInput'
import {
  CURRENCY_OPTIONS,
  emptyQuoteHeaderValues,
  headerToNewQuoteFields,
  quoteHeaderSchema,
} from './types'
import type { QuoteHeaderValues } from './types'

interface MeResponse {
  tenant: { default_currency: string }
}

export function QuoteNew() {
  const { t } = useTranslation('quotes')
  const navigate = useNavigate()
  const api = useApi()
  const queryClient = useQueryClient()
  const [formError, setFormError] = useState<string | null>(null)

  const { data: me, isLoading } = useQuery({
    queryKey: ['me'],
    queryFn: () => api<MeResponse>('/api/me'),
  })

  const form = useForm<QuoteHeaderValues>({
    initialValues: emptyQuoteHeaderValues(me?.tenant.default_currency ?? 'EUR'),
    validate: zodResolver(quoteHeaderSchema),
  })

  const mutation = useMutation({
    mutationFn: (values: QuoteHeaderValues) => createQuote(api, { ...headerToNewQuoteFields(values), lines: [] }),
    onSuccess: (quote) => {
      void queryClient.invalidateQueries({ queryKey: quotesKeys.all })
      void navigate({ to: '/quotes/$id', params: { id: quote.id } })
    },
    onError: (err) => {
      if (err instanceof ApiError && err.code === 'validation_failed' && err.details?.customer_id) {
        form.setFieldError('customerId', t('errors.party_required'))
      } else {
        setFormError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    },
  })

  if (isLoading || !me) {
    return <Loader />
  }

  const currency = form.values.currency.toUpperCase()
  const currencyOptions = CURRENCY_OPTIONS.includes(currency as (typeof CURRENCY_OPTIONS)[number])
    ? [...CURRENCY_OPTIONS]
    : [...CURRENCY_OPTIONS, currency]

  const handleSubmit = form.onSubmit((values) => {
    setFormError(null)
    mutation.mutate(values)
  })

  return (
    <form onSubmit={handleSubmit}>
      <Stack maw={560}>
        <PageHeader
          sticky
          breadcrumb={[t('list.title')]}
          title={t('create.title')}
          actions={
            <>
              <Button variant="subtle" onClick={() => navigate({ to: '/quotes' })} disabled={mutation.isPending}>
                {t('form.cancel')}
              </Button>
              <Button type="submit" loading={mutation.isPending}>
                {t('create.submit')}
              </Button>
            </>
          }
        />
        {formError && <Alert color="red">{formError}</Alert>}
        <PartyInput form={form} />
        <Select label={t('fields.currency')} withAsterisk data={currencyOptions} {...form.getInputProps('currency')} />
      </Stack>
    </form>
  )
}
