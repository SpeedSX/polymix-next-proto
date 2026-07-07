export interface Money {
  amount_minor: number
  currency: string
}

function minorUnitDigits(currency: string, locale: string): number {
  return new Intl.NumberFormat(locale, { style: 'currency', currency }).resolvedOptions().maximumFractionDigits ?? 2
}

export function formatMoney(money: Money, locale = 'en'): string {
  const digits = minorUnitDigits(money.currency, locale)
  return new Intl.NumberFormat(locale, { style: 'currency', currency: money.currency }).format(
    money.amount_minor / 10 ** digits,
  )
}

export function toMinorUnits(decimal: string, currency: string, locale = 'en'): number {
  const digits = minorUnitDigits(currency, locale)
  const value = Number.parseFloat(decimal || '0')
  return Math.round(value * 10 ** digits)
}

export function fromMinorUnits(amountMinor: number, currency: string, locale = 'en'): string {
  const digits = minorUnitDigits(currency, locale)
  return (amountMinor / 10 ** digits).toFixed(digits)
}
