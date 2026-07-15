import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { CustomerForm } from './Form'
import { emptyCustomerFormValues } from './types'
import type { Customer } from './types'

function renderForm(props: Partial<React.ComponentProps<typeof CustomerForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const { container } = render(
    <MantineProvider>
      <CustomerForm
        initialValues={emptyCustomerFormValues('EUR')}
        onSubmit={onSubmit}
        onSuccess={onSuccess}
        onCancel={onCancel}
        {...props}
      />
    </MantineProvider>,
  )

  const getField = (path: string) => container.querySelector<HTMLInputElement>(`[data-path="${path}"]`)!

  return { onSubmit, onSuccess, onCancel, getField }
}

function customer(overrides: Partial<Customer> = {}): Customer {
  return {
    id: '01',
    number: '000001',
    kind: 0,
    name: 'Adamant Print GmbH',
    legal_name: null,
    edrpou: null,
    tax_id: null,
    vat_ipn: null,
    status: 1,
    tags: [],
    industry: null,
    source: null,
    website: null,
    contacts: [],
    legal_address: null,
    delivery_address: null,
    payment_terms_days: 0,
    credit_limit: null,
    default_currency: 'EUR',
    default_discount_bp: 0,
    iban: null,
    bank_name: null,
    notes: null,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

describe('CustomerForm', () => {
  it('blocks submission when the required name field is empty', async () => {
    const { onSubmit } = renderForm()

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('This field is required.')).toBeInTheDocument()
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('renders API validation errors on the matching field', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(422, { code: 'validation_failed', message: 'bad', details: { legal_name: 'too long' } }),
    )
    const { getField } = renderForm({ onSubmit })

    fireEvent.change(getField('name'), { target: { value: 'Adamant Print GmbH' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('too long')).toBeInTheDocument()
  })

  it('calls onSuccess with the created customer', async () => {
    const created = customer()
    const onSubmit = vi.fn().mockResolvedValue(created)
    const { onSuccess, getField } = renderForm({ onSubmit })

    fireEvent.change(getField('name'), { target: { value: 'Adamant Print GmbH' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSuccess).toHaveBeenCalledWith(created))
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ name: 'Adamant Print GmbH' }))
  })
})
