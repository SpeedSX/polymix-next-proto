import { Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'

export function HomePage() {
  const { t } = useTranslation()
  return <Text>{t('app.title')}</Text>
}
