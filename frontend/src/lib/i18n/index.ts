import i18n from 'i18next'
import ICU from 'i18next-icu'
import { initReactI18next } from 'react-i18next'

import enCommon from './locales/en/common.json'
import enCustomers from './locales/en/customers.json'
import enInvoices from './locales/en/invoices.json'
import enOrders from './locales/en/orders.json'
import enSearch from './locales/en/search.json'
import uaCommon from './locales/ua/common.json'
import uaCustomers from './locales/ua/customers.json'
import uaInvoices from './locales/ua/invoices.json'
import uaOrders from './locales/ua/orders.json'
import uaSearch from './locales/ua/search.json'

export const SUPPORTED_LANGUAGES = ['en', 'ua'] as const
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number]

// Where the language switcher persists the user's choice (PLAN.md: "locale
// persisted per user (localStorage for the prototype)"). Only ever
// *restores* a choice the user actually made — the app's own default stays
// 'en' regardless of what PLAN.md's architecture section says about `ua`
// being the eventual default; see docs/adr/0007 for why (flipping the boot
// language breaks every existing English-asserting test).
export const LANGUAGE_STORAGE_KEY = 'polymix:lang'

function isSupportedLanguage(value: string | null): value is SupportedLanguage {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(value ?? '')
}

function restoredLanguage(): SupportedLanguage {
  try {
    const stored = localStorage.getItem(LANGUAGE_STORAGE_KEY)
    return isSupportedLanguage(stored) ? stored : 'en'
  } catch {
    return 'en'
  }
}

void i18n
  .use(ICU)
  .use(initReactI18next)
  .init({
    lng: restoredLanguage(),
    fallbackLng: 'en',
    defaultNS: 'common',
    ns: ['common', 'customers', 'orders', 'invoices', 'search'],
    resources: {
      en: { common: enCommon, customers: enCustomers, orders: enOrders, invoices: enInvoices, search: enSearch },
      ua: { common: uaCommon, customers: uaCustomers, orders: uaOrders, invoices: uaInvoices, search: uaSearch },
    },
    interpolation: {
      escapeValue: false,
    },
  })

export default i18n
