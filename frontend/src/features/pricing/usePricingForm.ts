import { useState } from 'react'
import { useForm, zodResolver } from '@mantine/form'
import type { UseFormReturnType } from '@mantine/form'
import { useTranslation } from 'react-i18next'
import type { z } from 'zod'

import { ApiError, apiErrorMessage, validationMessage } from '../../lib/api'
import { mapApiErrorField } from './types'
import type { CatalogDoc } from './types'

interface UsePricingFormOptions<Values extends Record<string, unknown>> {
  schema: z.ZodType<unknown, z.ZodTypeDef, unknown>
  initialValues: Values
  toDoc: (values: Values) => CatalogDoc
  onSubmit: (doc: CatalogDoc) => Promise<CatalogDoc>
  onSuccess: (doc: CatalogDoc) => void
}

export interface PricingFormState<Values extends Record<string, unknown>> {
  form: UseFormReturnType<Values>
  submitting: boolean
  formError: string | null
  submit: (event?: React.FormEvent<HTMLFormElement>) => void
}

/**
 * Wiring shared by every catalog form: Zod validation, submit, and mapping the
 * server's `validation_failed` field errors back onto form fields (a `_`
 * whole-document error becomes a form-level message).
 */
export function usePricingForm<Values extends Record<string, unknown>>(
  options: UsePricingFormOptions<Values>,
): PricingFormState<Values> {
  const { t } = useTranslation('pricing')
  const [submitting, setSubmitting] = useState(false)
  const [formError, setFormError] = useState<string | null>(null)
  const form = useForm<Values>({
    initialValues: options.initialValues,
    validate: zodResolver(options.schema),
  })

  const submit = form.onSubmit(async (values) => {
    setFormError(null)
    setSubmitting(true)
    try {
      const saved = await options.onSubmit(options.toDoc(values))
      options.onSuccess(saved)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'validation_failed' && err.details) {
        for (const [field, fieldError] of Object.entries(err.details)) {
          const path = mapApiErrorField(field)
          if (path === '') {
            setFormError(validationMessage(fieldError, t))
          } else {
            form.setFieldError(path, validationMessage(fieldError, t))
          }
        }
      } else {
        setFormError(apiErrorMessage(err, t, 'form.unexpectedError'))
      }
    } finally {
      setSubmitting(false)
    }
  })

  return { form, submitting, formError, submit }
}
