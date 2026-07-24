import { useTranslation } from 'react-i18next'

import { QUOTE_STATUS_META } from './types'
import type { QuoteStatusId, QuoteStatusMeta } from './types'

/** Quote statuses are a fixed, code-driven lifecycle (no backend dictionary
 * endpoint like orders): labels come from the `quotes` i18n namespace, tone and
 * allowed transitions from [`QUOTE_STATUS_META`]. */
export function useQuoteStatus() {
  const { t } = useTranslation('quotes')

  const labelFor = (id: QuoteStatusId): string => t(`status.${QUOTE_STATUS_META[id].key}`)
  const metaFor = (id: QuoteStatusId): QuoteStatusMeta => QUOTE_STATUS_META[id]

  const options = (Object.values(QUOTE_STATUS_META) as QuoteStatusMeta[]).map((meta) => ({
    value: String(meta.id),
    label: labelFor(meta.id),
  }))

  return { labelFor, metaFor, options }
}
