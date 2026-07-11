import type { TFunction } from 'i18next'

/** Shape of a single entry in a `validation_failed` error's `details` map — mirrors the API's `FieldError`. */
export interface ApiFieldError {
  code: string
  params?: Record<string, string>
}

function isApiFieldError(value: unknown): value is ApiFieldError {
  return typeof value === 'object' && value !== null && typeof (value as { code?: unknown }).code === 'string'
}

/**
 * Translates a field-level validation error from the API's `validation_failed`
 * `details` map via `common:validation.<code>`, interpolating any `params`
 * (e.g. the invalid value) into the translated message. Falls back to the
 * raw code when no translation exists yet, mirroring `apiErrorMessage`'s
 * fallback-to-identifier behavior.
 */
export function validationMessage(fieldError: unknown, t: TFunction): string {
  if (!isApiFieldError(fieldError)) {
    return String(fieldError)
  }
  return t(`common:validation.${fieldError.code}`, { defaultValue: fieldError.code, ...fieldError.params })
}
