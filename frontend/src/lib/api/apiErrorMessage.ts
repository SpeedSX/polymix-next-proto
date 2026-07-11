import type { TFunction } from 'i18next'

import { ApiError } from './ApiError'

/**
 * Translates an API error via its stable `code` (e.g. "order_has_invoice"),
 * looked up as `errors.<code>` in the caller's own i18n namespace. Falls
 * back to `fallbackKey` for non-`ApiError` failures, or to the API's
 * English `message` when the caller's namespace has no translation for
 * that code yet — the localized fallback is `err.message`, not `err.code`.
 */
export function apiErrorMessage(err: unknown, t: TFunction, fallbackKey: string): string {
  if (!(err instanceof ApiError)) {
    return t(fallbackKey)
  }
  return t(`errors.${err.code}`, { defaultValue: err.message, ...err.details })
}
