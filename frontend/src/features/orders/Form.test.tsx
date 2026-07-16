import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { AuthContext } from '../../lib/auth/context'
import type { Customer } from '../customers/types'
import { OrderForm } from './Form'
import { emptyOrderFormValues } from './types'

vi.mock('../customers/api', () => ({
  fetchCustomers: vi.fn(),
  fetchCustomer: vi.fn(),
  fetchCustomerStatusDictionary: vi.fn(),
  customersKeys: {
    all: ['customers'],
    list: (params: unknown) => ['customers', params],
    detail: (id: string) => ['customers', id],
    statusDictionary: () => ['dictionaries', 'customer-statuses'],
  },
}))

const { fetchCustomer, fetchCustomers, fetchCustomerStatusDictionary } = await import('../customers/api')

const CUSTOMER: Customer = {
  id: 'customer1',
  kind: 0,
  name: 'Acme Print',
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
}

function renderForm(props: Partial<React.ComponentProps<typeof OrderForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const queryClient = new QueryClient()

  const { container } = render(
    <MantineProvider>
      <QueryClientProvider client={queryClient}>
        <AuthContext.Provider value={{ mode: 'dev', orgId: 'org1', getToken: async () => 'token', signOut: () => {} }}>
          <OrderForm
            initialValues={emptyOrderFormValues('USD')}
            onSubmit={onSubmit}
            onSuccess={onSuccess}
            onCancel={onCancel}
            {...props}
          />
        </AuthContext.Provider>
      </QueryClientProvider>
    </MantineProvider>,
  )

  const getField = (path: string) => container.querySelector<HTMLInputElement>(`[data-path="${path}"]`)!

  return { onSubmit, onSuccess, onCancel, getField }
}

async function selectCustomer(getField: (path: string) => HTMLInputElement) {
  const input = getField('customerId')
  fireEvent.click(input)
  fireEvent.focus(input)
  fireEvent.click(await screen.findByText('Acme Print'))
}

describe('OrderForm', () => {
  beforeEach(() => {
    vi.mocked(fetchCustomers).mockResolvedValue({ items: [CUSTOMER], total: 1, page: 1, limit: 20 })
    vi.mocked(fetchCustomer).mockResolvedValue(CUSTOMER)
    vi.mocked(fetchCustomerStatusDictionary).mockResolvedValue({
      items: [{ id: 1, key: 'active', sort: 1, color: 'green', can_order: true, allowed_targets: [], labels: { en: 'Active' } }],
    })
  })

  it('lets the user search and pick a customer instead of typing an id', async () => {
    const onSubmit = vi.fn().mockResolvedValue({})
    const { getField } = renderForm({ onSubmit })

    await selectCustomer(getField)
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled())
    expect(onSubmit.mock.calls[0][0]).toMatchObject({ customer_id: 'customer1' })
  })

  it('rejects submission when no customer is selected', async () => {
    const { onSubmit, getField } = renderForm()

    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('This field is required.')).toBeInTheDocument()
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('blocks submission when unit price is not a decimal amount', async () => {
    const { onSubmit, getField } = renderForm()

    await selectCustomer(getField)
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: 'abc' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('Must be a valid decimal amount.')).toBeInTheDocument()
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('renders API validation errors on the matching line-item field', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(422, {
        code: 'validation_failed',
        message: 'bad',
        details: { 'line_items[0].quantity': 'must be greater than zero' },
      }),
    )
    const { getField } = renderForm({ onSubmit })

    await selectCustomer(getField)
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('must be greater than zero')).toBeInTheDocument()
  })

  it('maps a nested unit_price error onto the unitPrice field', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(422, {
        code: 'validation_failed',
        message: 'bad',
        details: { 'line_items[0].unit_price.currency': 'must be an ISO 4217 code' },
      }),
    )
    const { getField } = renderForm({ onSubmit })

    await selectCustomer(getField)
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('must be an ISO 4217 code')).toBeInTheDocument()
  })

  it('shows a form-level alert for a whole-array error instead of dropping it silently', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(422, {
        code: 'validation_failed',
        message: 'bad',
        details: { line_items: 'unit_price currency must match the order currency (USD)' },
      }),
    )
    const { getField } = renderForm({ onSubmit })

    await selectCustomer(getField)
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('unit_price currency must match the order currency (USD)')).toBeInTheDocument()
  })
})
