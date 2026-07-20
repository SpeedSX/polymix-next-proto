import { ActionIcon, Group, Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'

type ListPaginationProps = {
  page: number
  pageSize: number
  total: number
  onChange: (page: number) => void
}

type ChevronIconProps = {
  direction: 'left' | 'right'
}

function ChevronIcon({ direction }: ChevronIconProps) {
  const path = direction === 'left' ? 'm15 18-6-6 6-6' : 'm9 18 6-6-6-6'

  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      style={{ display: 'block' }}
    >
      <path d={path} stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ListPagination({ page, pageSize, total, onChange }: ListPaginationProps) {
  const { t } = useTranslation('common')
  if (total === 0) {
    return null
  }

  const totalPages = Math.max(1, Math.ceil(total / pageSize))
  const from = (page - 1) * pageSize + 1
  const to = Math.min(page * pageSize, total)

  return (
    <Group gap="sm" align="center">
      <Text size="sm" c="dimmed">
        {t('pagination.range', { from, to, total })}
      </Text>
      <Group gap={6}>
        <ActionIcon
          variant="default"
          size="lg"
          aria-label={t('pagination.back')}
          disabled={page <= 1}
          onClick={() => onChange(page - 1)}
        >
          <ChevronIcon direction="left" />
        </ActionIcon>
        <ActionIcon
          variant="default"
          size="lg"
          aria-label={t('pagination.forward')}
          disabled={page >= totalPages}
          onClick={() => onChange(page + 1)}
        >
          <ChevronIcon direction="right" />
        </ActionIcon>
      </Group>
    </Group>
  )
}
