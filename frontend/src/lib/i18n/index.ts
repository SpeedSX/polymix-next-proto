import i18n from 'i18next'
import ICU from 'i18next-icu'
import { initReactI18next } from 'react-i18next'
import { z } from 'zod'

import enCommon from './locales/en/common.json'
import enCustomers from './locales/en/customers.json'
import enInvoices from './locales/en/invoices.json'
import enOrders from './locales/en/orders.json'
import enSearch from './locales/en/search.json'
import enSettings from './locales/en/settings.json'
import ukCommon from './locales/uk/common.json'
import ukCustomers from './locales/uk/customers.json'
import ukInvoices from './locales/uk/invoices.json'
import ukOrders from './locales/uk/orders.json'
import ukSearch from './locales/uk/search.json'
import ukSettings from './locales/uk/settings.json'
import { zodErrorMap } from './zodErrorMap'

export const SUPPORTED_LANGUAGES = ['en', 'uk'] as const
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number]

// Where the language switcher persists the user's choice (PLAN.md: "locale
// persisted per user (localStorage for the prototype)"). Only ever
// *restores* a choice the user actually made — the app's own default stays
// 'en' regardless of what PLAN.md's architecture section says about `uk`
// being the eventual default; see docs/adr/0007 for why (flipping the boot
// language breaks every existing English-asserting test).
export const LANGUAGE_STORAGE_KEY = 'polymix:lang'

function isSupportedLanguage(value: string | null): value is SupportedLanguage {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(value ?? '')
}

function restoredLanguage(): SupportedLanguage {
  try {
    const stored = localStorage.getItem(LANGUAGE_STORAGE_KEY)
    // Pre-rename installs stored `ua`; treat it as the BCP-47 `uk` tag.
    if (stored === 'ua') {
      localStorage.setItem(LANGUAGE_STORAGE_KEY, 'uk')
      return 'uk'
    }
    return isSupportedLanguage(stored) ? stored : 'en'
  } catch {
    return 'en'
  }
}

i18n
  .use(ICU)
  .use(initReactI18next)
  .init({
    lng: restoredLanguage(),
    fallbackLng: 'en',
    defaultNS: 'common',
    ns: ['common', 'customers', 'orders', 'invoices', 'search', 'settings'],
    resources: {
      en: {
        common: enCommon,
        customers: enCustomers,
        orders: enOrders,
        invoices: enInvoices,
        search: enSearch,
        settings: enSettings,
      },
      uk: {
        common: ukCommon,
        customers: ukCustomers,
        orders: ukOrders,
        invoices: ukInvoices,
        search: ukSearch,
        settings: ukSettings,
      },
    },
    interpolation: {
      escapeValue: false,
    },
  })
  // A rejected init leaves the app rendering raw translation keys; surface it
  // instead of leaving a dead, silent promise.
  .catch((error: unknown) => {
    console.error('i18n initialization failed', error)
  })

z.setErrorMap(zodErrorMap)

export default i18n
