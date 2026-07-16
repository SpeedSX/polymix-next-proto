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
  const points = direction === 'left' ? '9.5 5.5 5.5 9.5 9.5 13.5' : '6.5 5.5 10.5 9.5 6.5 13.5'

  return (
    <svg
      width="16"
      height="16"
      viewBox="0 2 16 16"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
      style={{ display: 'block' }}
    >
      <polyline
        points={points}
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
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
          variant="light"
          color="gray"
          radius="xl"
          size="lg"
          aria-label={t('pagination.back')}
          disabled={page <= 1}
          onClick={() => onChange(page - 1)}
        >
          <ChevronIcon direction="left" />
        </ActionIcon>
        <ActionIcon
          variant="light"
          color="gray"
          radius="xl"
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
