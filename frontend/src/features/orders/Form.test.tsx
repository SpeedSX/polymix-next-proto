import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { OrderForm } from './Form'
import { emptyOrderFormValues } from './types'

function renderForm(props: Partial<React.ComponentProps<typeof OrderForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const { container } = render(
    <MantineProvider>
      <OrderForm
        initialValues={emptyOrderFormValues('USD')}
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

describe('OrderForm', () => {
  it('blocks submission when unit price is not a decimal amount', async () => {
    const { onSubmit, getField } = renderForm()

    fireEvent.change(getField('customerId'), { target: { value: 'customer1' } })
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: 'abc' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('Invalid')).toBeInTheDocument()
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

    fireEvent.change(getField('customerId'), { target: { value: 'customer1' } })
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

    fireEvent.change(getField('customerId'), { target: { value: 'customer1' } })
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

    fireEvent.change(getField('customerId'), { target: { value: 'customer1' } })
    fireEvent.change(getField('lineItems.0.description'), { target: { value: 'Business cards' } })
    fireEvent.change(getField('lineItems.0.unitPrice'), { target: { value: '10.00' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('unit_price currency must match the order currency (USD)')).toBeInTheDocument()
  })
})
