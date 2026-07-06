import { AppShell as MantineAppShell, Group, NavLink, Title } from '@mantine/core'
import { useTranslation } from 'react-i18next'
import { Outlet } from '@tanstack/react-router'

export function AppShell() {
  const { t } = useTranslation()

  return (
    <MantineAppShell header={{ height: 60 }} navbar={{ width: 220, breakpoint: 'sm' }} padding="md">
      <MantineAppShell.Header>
        <Group h="100%" px="md">
          <Title order={3}>{t('app.title')}</Title>
        </Group>
      </MantineAppShell.Header>
      <MantineAppShell.Navbar p="md">
        <NavLink label={t('nav.customers')} disabled />
        <NavLink label={t('nav.orders')} disabled />
        <NavLink label={t('nav.invoices')} disabled />
      </MantineAppShell.Navbar>
      <MantineAppShell.Main>
        <Outlet />
      </MantineAppShell.Main>
    </MantineAppShell>
  )
}
