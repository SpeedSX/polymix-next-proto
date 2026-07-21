import type { CSSProperties } from 'react'
import { Box, Stack, Text } from '@mantine/core'
import { useTranslation } from 'react-i18next'
import { Link, Outlet } from '@tanstack/react-router'

const HAIRLINE = '1px solid var(--mantine-color-gray-5)'

interface SettingsNavLinkProps {
  to: string
  label: string
}

function SettingsNavLink({ to, label }: SettingsNavLinkProps) {
  const active: CSSProperties = {
    color: 'var(--mantine-primary-color-filled)',
    background: 'var(--mantine-primary-color-light)',
    boxShadow: 'inset 2px 0 0 var(--mantine-primary-color-filled)',
  }

  return (
    <Link
      to={to}
      activeProps={{ style: active }}
      style={{
        padding: '9px 12px',
        color: 'var(--mantine-color-text)',
        textDecoration: 'none',
        fontFamily: 'var(--mantine-font-family-headings)',
        fontWeight: 500,
        fontSize: 14,
      }}
    >
      {label}
    </Link>
  )
}

export function SettingsLayout() {
  const { t } = useTranslation('settings')

  return (
    <Box style={{ display: 'flex', minHeight: '100%' }}>
      <Box component="aside" w={190} style={{ flex: 'none', borderRight: HAIRLINE, padding: '22px 14px' }}>
        <Stack gap={2}>
          <Text
            size="xs"
            c="dimmed"
            tt="uppercase"
            style={{ letterSpacing: '0.08em', padding: '0 12px 10px' }}
          >
            {t('title')}
          </Text>
          <SettingsNavLink to="/settings/catalog" label={t('nav.catalog')} />
          <SettingsNavLink to="/settings/users-roles" label={t('nav.usersRoles')} />
        </Stack>
      </Box>
      <Box style={{ flex: 1, minWidth: 0, padding: 'var(--mantine-spacing-md)' }}>
        <Outlet />
      </Box>
    </Box>
  )
}
