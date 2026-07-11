import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { InvoiceForm } from './Form'
import { fromInvoice } from './types'
import type { Invoice } from './types'

const DRAFT_INVOICE: Invoice = {
  id: 'invoice1',
  number: 'INV-000001',
  order_id: 'order1',
  customer_id: 'customer1',
  status: 'draft',
  currency: 'USD',
  exchange_rate: null,
  line_items: [{ description: 'Business cards', quantity: 3, unit_price: { amount_minor: 250, currency: 'USD' } }],
  net_total: { amount_minor: 750, currency: 'USD' },
  tax_rate_bp: 1900,
  tax_total: { amount_minor: 143, currency: 'USD' },
  gross_total: { amount_minor: 893, currency: 'USD' },
  issue_date: null,
  due_date: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
}

function renderForm(props: Partial<React.ComponentProps<typeof InvoiceForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const { container } = render(
    <MantineProvider>
      <InvoiceForm
        initialValues={fromInvoice(DRAFT_INVOICE)}
        currency={DRAFT_INVOICE.currency}
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

describe('InvoiceForm', () => {
  it('blocks submission when unit price is not a decimal amount', async () => {
    const { onSubmit, getField } = renderForm()

    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: 'abc' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('Invalid')).toBeInTheDocument()
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('submits the recomputed line items', async () => {
    const onSubmit = vi.fn().mockResolvedValue(DRAFT_INVOICE)
    const { getField } = renderForm({ onSubmit })

    fireEvent.change(getField('lineItems.0.quantity'), { target: { value: '10' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSubmit).toHaveBeenCalled())
    expect(onSubmit).toHaveBeenCalledWith({
      line_items: [{ description: 'Business cards', quantity: 10, unit_price: { amount_minor: 250, currency: 'USD' } }],
    })
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

    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('must be greater than zero')).toBeInTheDocument()
  })

  it('shows a form-level alert for a conflict error instead of dropping it silently', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(409, {
        code: 'invoice_not_draft',
        message: 'invoice can only be edited while in draft status; void and reissue instead',
      }),
    )
    const { getField } = renderForm({ onSubmit })

    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(
      await screen.findByText('This invoice can only be edited while in draft status; void and reissue it instead.'),
    ).toBeInTheDocument()
  })
})
