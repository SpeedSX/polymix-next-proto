import { useState } from 'react'
import { AppShell as MantineAppShell, Button, Group, Kbd, NavLink, Text, Title } from '@mantine/core'
import { useHotkeys } from '@mantine/hooks'
import { OrganizationSwitcher } from '@clerk/react'
import { useTranslation } from 'react-i18next'
import { Link, Outlet } from '@tanstack/react-router'

import { useAuth } from '../lib/auth'
import { Omnibox } from '../features/search/Omnibox'

export function AppShell() {
  const { t } = useTranslation()
  const { t: tSearch } = useTranslation('search')
  const { mode, orgId, signOut } = useAuth()
  const [searchOpened, setSearchOpened] = useState(false)

  useHotkeys([['mod+K', () => setSearchOpened(true)]])

  return (
    <MantineAppShell header={{ height: 60 }} navbar={{ width: 220, breakpoint: 'sm' }} padding="md">
      <MantineAppShell.Header>
        <Group h="100%" px="md" justify="space-between">
          <Title order={3}>{t('app.title')}</Title>
          <Group>
            <Button variant="default" onClick={() => setSearchOpened(true)}>
              {tSearch('trigger')} <Kbd ml={6}>Ctrl+K</Kbd>
            </Button>
            {mode === 'clerk' ? (
              <OrganizationSwitcher hidePersonal />
            ) : (
              <Text size="sm" c="dimmed">
                {orgId}
              </Text>
            )}
            <Button variant="subtle" onClick={signOut}>
              {t('auth.signOut')}
            </Button>
          </Group>
        </Group>
      </MantineAppShell.Header>
      <MantineAppShell.Navbar p="md">
        <NavLink label={t('nav.customers')} component={Link} to="/customers" />
        <NavLink label={t('nav.orders')} component={Link} to="/orders" />
        <NavLink label={t('nav.invoices')} component={Link} to="/invoices" />
      </MantineAppShell.Navbar>
      <MantineAppShell.Main>
        <Outlet />
      </MantineAppShell.Main>
      <Omnibox opened={searchOpened} onClose={() => setSearchOpened(false)} />
    </MantineAppShell>
  )
}
