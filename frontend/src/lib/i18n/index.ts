import i18n from 'i18next'
import ICU from 'i18next-icu'
import { initReactI18next } from 'react-i18next'

import common from './locales/en/common.json'
import customers from './locales/en/customers.json'
import invoices from './locales/en/invoices.json'
import orders from './locales/en/orders.json'

void i18n
  .use(ICU)
  .use(initReactI18next)
  .init({
    lng: 'en',
    fallbackLng: 'en',
    defaultNS: 'common',
    ns: ['common', 'customers', 'orders', 'invoices'],
    resources: {
      en: { common, customers, orders, invoices },
    },
    interpolation: {
      escapeValue: false,
    },
  })

export default i18n
