import { Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'

export function SettingsCatalog() {
  const { t } = useTranslation('settings')

  return (
    <Text c="dimmed" size="sm">
      {t('catalog.empty')}
    </Text>
  )
}
