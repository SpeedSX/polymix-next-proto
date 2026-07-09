export interface Money {
  amount_minor: number
  currency: string
}

function minorUnitDigits(currency: string, locale: string): number {
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency }).resolvedOptions().maximumFractionDigits ?? 2
  } catch {
    return 2
  }
}

export function formatMoney(money: Money, locale = 'en'): string {
  const digits = minorUnitDigits(money.currency, locale)
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency: money.currency }).format(
      money.amount_minor / 10 ** digits,
    )
  } catch {
    return (money.amount_minor / 10 ** digits).toFixed(digits)
  }
}

// Matches a plain decimal amount with a single '.' or ',' decimal separator
// (both are common across the app's locales — 'en' uses '.', 'ua'/'de' use
// ',') and no thousands separator. Use to validate money text inputs before
// they reach toMinorUnits.
export const MONEY_DECIMAL_PATTERN = /^\d+([.,]\d+)?$/

export function toMinorUnits(decimal: string, currency: string, locale = 'en'): number {
  const digits = minorUnitDigits(currency, locale)
  // Input is constrained to MONEY_DECIMAL_PATTERN, so there's at most one
  // separator and replacing it with '.' can't collide with a thousands group.
  const normalized = decimal.trim().replace(',', '.')
  const value = Number.parseFloat(normalized || '0')
  return Math.round(value * 10 ** digits)
}

export function fromMinorUnits(amountMinor: number, currency: string, locale = 'en'): string {
  const digits = minorUnitDigits(currency, locale)
  return (amountMinor / 10 ** digits).toFixed(digits)
}
