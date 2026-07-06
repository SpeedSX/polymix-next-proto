import i18n from 'i18next'
import ICU from 'i18next-icu'
import { initReactI18next } from 'react-i18next'

import common from './locales/en/common.json'
import customers from './locales/en/customers.json'

void i18n
  .use(ICU)
  .use(initReactI18next)
  .init({
    lng: 'en',
    fallbackLng: 'en',
    defaultNS: 'common',
    ns: ['common', 'customers'],
    resources: {
      en: { common, customers },
    },
    interpolation: {
      escapeValue: false,
    },
  })

export default i18n
