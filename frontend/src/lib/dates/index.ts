// `locale` here is the i18next language tag ('en' | 'uk'), passed straight
// into Intl — same convention lib/money already uses.

export function formatDateTime(value: string, locale = 'en'): string {
  try {
    return new Intl.DateTimeFormat(locale, {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(value))
  } catch {
    return value
  }
}

// `value` is a date-only string ("2026-01-15") — parsed as UTC midnight so
// the displayed day never shifts with the viewer's timezone.
export function formatDate(value: string, locale = 'en'): string {
  try {
    return new Intl.DateTimeFormat(locale, { year: 'numeric', month: '2-digit', day: '2-digit' }).format(
      new Date(`${value}T00:00:00Z`),
    )
  } catch {
    return value
  }
}
