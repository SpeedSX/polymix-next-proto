import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { CustomerForm } from './Form'
import { emptyCustomerFormValues } from './types'

function renderForm(props: Partial<React.ComponentProps<typeof CustomerForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const { container } = render(
    <MantineProvider>
      <CustomerForm
        initialValues={emptyCustomerFormValues}
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

describe('CustomerForm', () => {
  it('blocks submission when the required name field is empty', async () => {
    const { onSubmit } = renderForm()

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('String must contain at least 1 character(s)')).toBeInTheDocument()
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('renders API validation errors on the matching field', async () => {
    const onSubmit = vi.fn().mockRejectedValue(
      new ApiError(422, { code: 'validation_failed', message: 'bad', details: { contact_name: 'too long' } }),
    )
    const { getField } = renderForm({ onSubmit })

    fireEvent.change(getField('name'), { target: { value: 'Adamant Print GmbH' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('too long')).toBeInTheDocument()
  })

  it('calls onSuccess with the created customer', async () => {
    const customer = {
      id: '01',
      name: 'Adamant Print GmbH',
      contact_name: null,
      email: null,
      phone: null,
      address: null,
      notes: null,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    }
    const onSubmit = vi.fn().mockResolvedValue(customer)
    const { onSuccess, getField } = renderForm({ onSubmit })

    fireEvent.change(getField('name'), { target: { value: 'Adamant Print GmbH' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSuccess).toHaveBeenCalledWith(customer))
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ name: 'Adamant Print GmbH' }))
  })
})
