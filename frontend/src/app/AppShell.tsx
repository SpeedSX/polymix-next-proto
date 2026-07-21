import { useState } from 'react'
import type { CSSProperties, ReactNode } from 'react'
import {
  ActionIcon,
  AppShell as MantineAppShell,
  Burger,
  Group,
  Kbd,
  Menu,
  Text,
  Tooltip,
  UnstyledButton,
} from '@mantine/core'
import { useDisclosure, useHotkeys, useMediaQuery } from '@mantine/hooks'
import {
  IconBuilding,
  IconChevronRight,
  IconClipboardList,
  IconFileInvoice,
  IconLayoutSidebarLeftCollapse,
  IconLayoutSidebarLeftExpand,
  IconLogout,
  IconSearch,
  IconUsers,
  IconWorld,
  type Icon,
} from '@tabler/icons-react'
import { OrganizationSwitcher } from '@clerk/react'
import { useTranslation } from 'react-i18next'
import { Link, Outlet } from '@tanstack/react-router'

import { useAuth } from '../lib/auth'
import { LANGUAGE_STORAGE_KEY, SUPPORTED_LANGUAGES } from '../lib/i18n'
import type { SupportedLanguage } from '../lib/i18n'
import { Omnibox } from '../features/search/Omnibox'

const RAIL_BG = 'var(--mantine-color-steel-9)'
const NAV_INACTIVE = '#c4d2e0'
const NAV_ACTIVE_BG = 'rgba(148,188,227,0.18)'
const NAV_ACCENT = '#94bce3'
const NAV_DIVIDER = 'rgba(255,255,255,0.12)'
const NAV_WIDTH_EXPANDED = 220
const NAV_WIDTH_COLLAPSED = 68

/** The PolyMix rounded-pixel-quad brand mark (docs/design). */
function PolyMixMark({ size = 30 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 120 120" aria-label="PolyMix">
      <rect x="8" y="8" width="48" height="48" rx="15" fill="#94bce3" />
      <rect x="64" y="8" width="48" height="48" rx="15" fill="#cfe4fa" />
      <rect x="8" y="64" width="48" height="48" rx="15" fill="#7e9cb8" />
      <rect x="64" y="64" width="48" height="48" rx="15" fill="#b5d9fd" />
    </svg>
  )
}

function railRowStyle(collapsed: boolean): CSSProperties {
  return {
    display: 'flex',
    alignItems: 'center',
    gap: 11,
    width: '100%',
    padding: collapsed ? 10 : '9px 12px',
    justifyContent: collapsed ? 'center' : 'flex-start',
    color: NAV_INACTIVE,
    textDecoration: 'none',
    fontFamily: 'var(--mantine-font-family-headings)',
    fontWeight: 500,
    fontSize: 14,
  }
}

interface NavItemProps {
  to: string
  label: string
  icon: Icon
  collapsed: boolean
  onNavigate: () => void
}

function NavItem({ to, label, icon: IconComponent, collapsed, onNavigate }: NavItemProps) {
  const active: CSSProperties = {
    color: '#ffffff',
    background: NAV_ACTIVE_BG,
    boxShadow: `inset 2px 0 0 ${NAV_ACCENT}`,
  }

  return (
    <Tooltip label={label} position="right" disabled={!collapsed} withArrow>
      <Link
        to={to}
        className="rail-row"
        style={railRowStyle(collapsed)}
        activeProps={{ style: active }}
        onClick={onNavigate}
      >
        <IconComponent size={18} stroke={1.5} />
        {!collapsed && <span>{label}</span>}
      </Link>
    </Tooltip>
  )
}

interface RailButtonProps {
  label: string
  icon: Icon
  collapsed: boolean
  onClick?: () => void
  trailing?: ReactNode
}

function RailButton({ label, icon: IconComponent, collapsed, onClick, trailing }: RailButtonProps) {
  return (
    <Tooltip label={label} position="right" disabled={!collapsed} withArrow>
      <UnstyledButton
        className="rail-row"
        style={railRowStyle(collapsed)}
        onClick={onClick}
        aria-label={label}
      >
        <IconComponent size={18} stroke={1.5} />
        {!collapsed && <span style={{ flex: 1, textAlign: 'left' }}>{label}</span>}
        {!collapsed && trailing}
      </UnstyledButton>
    </Tooltip>
  )
}

function LanguageSwitcher({ collapsed }: { collapsed: boolean }) {
  const { t, i18n } = useTranslation()

  const change = (lng: SupportedLanguage) => {
    void i18n.changeLanguage(lng)
    try {
      localStorage.setItem(LANGUAGE_STORAGE_KEY, lng)
    } catch {
      // localStorage can throw in locked-down environments (private
      // browsing, disabled storage) — the switch itself still works,
      // it just won't survive a reload.
    }
  }

  return (
    <Menu position="right-end" withArrow width={160}>
      <Menu.Target>
        <UnstyledButton
          className="rail-row"
          style={railRowStyle(collapsed)}
          aria-label={t('lang.label')}
        >
          <IconWorld size={18} stroke={1.5} />
          {!collapsed && (
            <>
              <span style={{ flex: 1, textAlign: 'left' }}>{t(`lang.${i18n.language}`)}</span>
              <IconChevronRight size={14} stroke={1.5} />
            </>
          )}
        </UnstyledButton>
      </Menu.Target>
      <Menu.Dropdown>
        {SUPPORTED_LANGUAGES.map((lng) => (
          <Menu.Item
            key={lng}
            onClick={() => change(lng)}
            fw={i18n.language === lng ? 500 : 400}
          >
            {t(`lang.${lng}`)}
          </Menu.Item>
        ))}
      </Menu.Dropdown>
    </Menu>
  )
}

function OrgControl({ collapsed }: { collapsed: boolean }) {
  const { mode, orgId } = useAuth()

  if (mode === 'clerk') {
    return (
      <Group justify={collapsed ? 'center' : 'flex-start'} px={collapsed ? 0 : 12} py={4}>
        <OrganizationSwitcher hidePersonal />
      </Group>
    )
  }

  return (
    <Tooltip label={orgId} position="right" disabled={!collapsed} withArrow>
      <Group gap={11} style={railRowStyle(collapsed)} wrap="nowrap">
        <IconBuilding size={18} stroke={1.5} />
        {!collapsed && (
          <Text size="sm" c={NAV_INACTIVE} truncate>
            {orgId}
          </Text>
        )}
      </Group>
    </Tooltip>
  )
}

function Divider({ collapsed }: { collapsed: boolean }) {
  return (
    <div
      style={{
        height: 1,
        background: NAV_DIVIDER,
        margin: collapsed ? '10px 12px' : '10px 20px',
      }}
    />
  )
}

export function AppShell() {
  const { t } = useTranslation()
  const { t: tSearch } = useTranslation('search')
  const { signOut } = useAuth()
  const [searchOpened, setSearchOpened] = useState(false)
  const [mobileOpened, { toggle: toggleMobile, close: closeMobile }] = useDisclosure(false)
  const [desktopCollapsed, { toggle: toggleDesktop }] = useDisclosure(false)
  const isMobile = useMediaQuery('(max-width: 48em)')

  useHotkeys([['mod+K', () => setSearchOpened(true)]])

  const navItems: Array<{ to: string; label: string; icon: Icon }> = [
    { to: '/customers', label: t('nav.customers'), icon: IconUsers },
    { to: '/orders', label: t('nav.orders'), icon: IconClipboardList },
    { to: '/invoices', label: t('nav.invoices'), icon: IconFileInvoice },
  ]

  const openSearch = () => {
    setSearchOpened(true)
    closeMobile()
  }

  const ToggleIcon = desktopCollapsed ? IconLayoutSidebarLeftExpand : IconLayoutSidebarLeftCollapse

  return (
    <MantineAppShell
      header={{ height: 56, collapsed: !isMobile }}
      navbar={{
        width: desktopCollapsed ? NAV_WIDTH_COLLAPSED : NAV_WIDTH_EXPANDED,
        breakpoint: 'sm',
        collapsed: { mobile: !mobileOpened },
      }}
      padding="md"
    >
      <MantineAppShell.Header>
        <Group h="100%" px="md" gap="sm">
          <Burger opened={mobileOpened} onClick={toggleMobile} hiddenFrom="sm" size="sm" />
          <Text ff="heading" fw={500} fz={18} style={{ letterSpacing: '-0.02em' }}>
            Poly<span style={{ color: 'var(--mantine-color-steel-6)' }}>Mix</span>
          </Text>
        </Group>
      </MantineAppShell.Header>
      <MantineAppShell.Navbar
        p={0}
        style={{
          background: RAIL_BG,
          border: 'none',
          color: '#e8eef5',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <Group
          gap={11}
          h={60}
          px={desktopCollapsed ? 0 : 20}
          justify={desktopCollapsed ? 'center' : 'flex-start'}
          wrap="nowrap"
        >
          <PolyMixMark size={30} />
          {!desktopCollapsed && (
            <Text
              fz={20}
              c="#fff"
              style={{ fontFamily: '"Fira Sans Condensed", sans-serif', fontWeight: 700, letterSpacing: '-0.02em' }}
            >
              Poly<span style={{ color: '#b5d9fd' }}>Mix</span>
            </Text>
          )}
        </Group>
        <Divider collapsed={desktopCollapsed} />
        <nav style={{ display: 'flex', flexDirection: 'column', gap: 2, padding: '0 12px' }}>
          {navItems.map((item) => (
            <NavItem
              key={item.to}
              to={item.to}
              label={item.label}
              icon={item.icon}
              collapsed={desktopCollapsed}
              onNavigate={closeMobile}
            />
          ))}
        </nav>

        <div style={{ marginTop: 'auto' }}>
          <Divider collapsed={desktopCollapsed} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2, padding: '0 12px' }}>
            <RailButton
              label={tSearch('trigger')}
              icon={IconSearch}
              collapsed={desktopCollapsed}
              onClick={openSearch}
              trailing={<Kbd>Ctrl+K</Kbd>}
            />
            <LanguageSwitcher collapsed={desktopCollapsed} />
            <OrgControl collapsed={desktopCollapsed} />
            <RailButton
              label={t('auth.signOut')}
              icon={IconLogout}
              collapsed={desktopCollapsed}
              onClick={signOut}
            />
          </div>
          <Group
            p={12}
            justify={desktopCollapsed ? 'center' : 'flex-end'}
            visibleFrom="sm"
          >
            <Tooltip label={t('nav.toggleSidebar')} position="right" withArrow>
              <ActionIcon
                variant="subtle"
                onClick={toggleDesktop}
                aria-label={t('nav.toggleSidebar')}
                style={{ color: NAV_INACTIVE }}
              >
                <ToggleIcon size={20} stroke={1.5} />
              </ActionIcon>
            </Tooltip>
          </Group>
        </div>
      </MantineAppShell.Navbar>
      <MantineAppShell.Main>
        <Outlet />
      </MantineAppShell.Main>
      <Omnibox opened={searchOpened} onClose={() => setSearchOpened(false)} />
    </MantineAppShell>
  )
}
