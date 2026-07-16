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
// (both are common across the app's locales — 'en' uses '.', 'uk'/'de' use
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

// Display-only conversion of `money` into `targetCurrency` using a stored
// `rateBaseToQuote` snapshot ("1 targetCurrency = <rate> money.currency",
// per PLAN.md's exchange_rate shape — base is the tenant's default
// currency, quote is the invoice's currency). Never used for accounting;
// returns null when there's no snapshot to convert with.
export function convertedDisplay(money: Money, rateBaseToQuote: string | null, targetCurrency: string, locale = 'en'): string | null {
  if (!rateBaseToQuote) {
    return null
  }
  const rate = Number.parseFloat(rateBaseToQuote)
  if (!Number.isFinite(rate) || rate <= 0) {
    return null
  }
  const majorAmount = Number.parseFloat(fromMinorUnits(money.amount_minor, money.currency, locale))
  const converted = majorAmount / rate
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency: targetCurrency }).format(converted)
  } catch {
    return converted.toFixed(2)
  }
}
