import { fireEvent, render, screen } from '@testing-library/react'
import { MantineProvider } from '@mantine/core'
import { describe, expect, it, vi } from 'vitest'

import '../../lib/i18n'
import { ApiError } from '../../lib/api'
import { MaterialForm } from './MaterialForm'
import { emptyMaterialFormValues } from './types'
import type { CatalogDoc } from './types'

function renderForm(props: Partial<React.ComponentProps<typeof MaterialForm>> = {}) {
  const onSubmit = vi.fn()
  const onSuccess = vi.fn()
  const onCancel = vi.fn()

  const { container } = render(
    <MantineProvider>
      <MaterialForm
        breadcrumb={['Catalog', 'Material']}
        title="Material"
        initialValues={emptyMaterialFormValues}
        onSubmit={onSubmit}
        onSuccess={onSuccess}
        onCancel={onCancel}
        {...props}
      />
    </MantineProvider>,
  )

  const getField = (path: string) => container.querySelector<HTMLInputElement>(`[data-path="${path}"]`)
  return { onSubmit, onSuccess, onCancel, getField }
}

function fillValid(getField: (path: string) => HTMLInputElement | null) {
  fireEvent.change(getField('name')!, { target: { value: 'Munken Cream 90gsm' } })
  fireEvent.change(getField('kind')!, { target: { value: 'Text stock' } })
  fireEvent.change(getField('price')!, { target: { value: '0.041' } })
}

describe('MaterialForm', () => {
  it('blocks submission when the required name field is empty', async () => {
    const { onSubmit } = renderForm()

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    // Both name and kind are required, so the message appears more than once.
    expect((await screen.findAllByText('This field is required.')).length).toBeGreaterThan(0)
    expect(onSubmit).not.toHaveBeenCalled()
  })

  it('renders API validation errors on the matching field', async () => {
    const onSubmit = vi
      .fn()
      .mockRejectedValue(new ApiError(422, { code: 'validation_failed', message: 'bad', details: { pricing: 'price too low' } }))
    const { getField } = renderForm({ onSubmit })

    fillValid(getField)
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(await screen.findByText('price too low')).toBeInTheDocument()
  })

  it('calls onSuccess and submits per_sheet pricing', async () => {
    const created: CatalogDoc = { id: 'material:x', name: 'Munken Cream 90gsm' }
    const onSubmit = vi.fn().mockResolvedValue(created)
    const { onSuccess, getField } = renderForm({ onSubmit })

    fillValid(getField)
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await vi.waitFor(() => expect(onSuccess).toHaveBeenCalledWith(created))
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ pricing: expect.objectContaining({ basis: 'per_sheet', price_micro: 41000 }) }),
    )
  })

  it('hides the sheet-size fields when the basis is not per_sheet', () => {
    const { getField } = renderForm()

    expect(getField('sheetWidth')).not.toBeNull()

    fireEvent.click(screen.getByText('per_m2'))

    expect(getField('sheetWidth')).toBeNull()
    expect(getField('sheetHeight')).toBeNull()
  })
})
