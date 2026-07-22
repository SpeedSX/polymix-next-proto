import { useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { ActionIcon, Box, Button, Group, Kbd, Popover, Stack, Text, TextInput, Title, UnstyledButton } from '@mantine/core'
import { useHotkeys } from '@mantine/hooks'
import { IconDownload, IconFilter, IconSearch, IconX } from '@tabler/icons-react'
import { useTranslation } from 'react-i18next'

export interface ListTab {
  label: ReactNode
  /** Count shown, muted, after the label. */
  count?: number
}

export interface ListLayoutProps {
  title: ReactNode
  /** Muted secondary line under the title (e.g. a record count). */
  subtitle?: ReactNode
  /** Segment tabs on the left of the bar below the header; the active one is underlined. */
  tabs?: ListTab[]
  activeTab?: number
  onTabChange?: (index: number) => void
  searchValue?: string
  onSearchChange?: (value: string) => void
  searchPlaceholder?: string
  /** Filter controls rendered inside the Filter popover; the Filter button is hidden when absent. */
  filters?: ReactNode
  /** Active-filter count shown as a badge on the Filter button. */
  filterCount?: number
  onClearFilters?: () => void
  /** Export control; a no-op placeholder button renders whenever a handler is supplied. */
  onExport?: () => void
  /** Right-aligned primary control, e.g. a create button. */
  primaryAction?: ReactNode
  /** Rendered right-aligned in the bar below the header. */
  pagination?: ReactNode
  /** The grid; sits on a lighter panel that scrolls with the document. */
  children: ReactNode
}

const HAIRLINE = '1px solid var(--mantine-color-gray-5)'
// AppShell.Main wraps the page in `md` padding, framing it in body gray. Cancel
// it so the header band and white grid run edge to edge; content inside each
// region is re-inset with `px="md"` instead.
const NEG_MD = 'calc(var(--mantine-spacing-md) * -1)'

/**
 * The brand row height and divider offset in AppShell put the sidebar's
 * hairline 70px below the viewport top and the second nav item's top at
 * ~119px. The header title row (60px, centred) lands the page title on the
 * logo and its bottom border on the sidebar hairline; the ~48px pagination
 * bar carries the white panel top down to the second nav item. Keep these in
 * step with AppShell when either side changes.
 */
const TITLE_ROW_HEIGHT = 60
const TITLE_ROW_PADDING_BOTTOM = 10
const SUBBAR_HEIGHT = 48

/**
 * Total height of the sticky header band (title area + sub-bar), measured from
 * the viewport top. Grids pass this as the sticky-header offset so the table's
 * column header pins directly below the band.
 */
export const LIST_HEADER_HEIGHT = TITLE_ROW_HEIGHT + TITLE_ROW_PADDING_BOTTOM + 1 + SUBBAR_HEIGHT

export function ListLayout({
  title,
  subtitle,
  tabs,
  activeTab = 0,
  onTabChange,
  searchValue,
  onSearchChange,
  searchPlaceholder,
  filters,
  filterCount = 0,
  onClearFilters,
  onExport,
  primaryAction,
  pagination,
  children,
}: ListLayoutProps) {
  const { t } = useTranslation('common')
  const [filterOpen, setFilterOpen] = useState(false)
  const [searchFocused, setSearchFocused] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)

  useHotkeys([['/', () => searchRef.current?.focus()]])

  return (
    <Box style={{ marginLeft: NEG_MD, marginRight: NEG_MD, marginBottom: NEG_MD }}>
      <Box
        style={{
          position: 'sticky',
          top: 0,
          zIndex: 5,
          background: 'var(--mantine-color-body)',
          // AppShell adds `md` top padding; cancel it with a negative margin so
          // the band pins flush to the scroll-port top (no travel) AND its
          // content starts at viewport 0 — the 60px title row then centres the
          // page title on the sidebar logo, and the header hairline lands on the
          // sidebar divider.
          marginTop: NEG_MD,
        }}
      >
        <Box style={{ paddingBottom: TITLE_ROW_PADDING_BOTTOM, borderBottom: HAIRLINE }}>
          <Group
            justify="space-between"
            align="center"
            gap="md"
            wrap="nowrap"
            px="md"
            style={{ minHeight: TITLE_ROW_HEIGHT }}
          >
            <Box style={{ minWidth: 0 }}>
            <Title order={2} fz={24} lh={1.1} style={{ margin: 0 }}>
              {title}
            </Title>
            {subtitle && (
              <Text size="xs" c="dimmed" mt={2}>
                {subtitle}
              </Text>
            )}
          </Box>

          <Group gap="sm" wrap="nowrap" style={{ flex: 'none' }}>
            {onSearchChange && (
              <TextInput
                ref={searchRef}
                w={240}
                leftSection={<IconSearch size={15} stroke={1.5} />}
                placeholder={searchPlaceholder}
                value={searchValue}
                onChange={(event) => onSearchChange(event.currentTarget.value)}
                onFocus={() => setSearchFocused(true)}
                onBlur={() => setSearchFocused(false)}
                rightSection={
                  !searchFocused && !searchValue ? (
                    <Kbd size="xs" c="dimmed">
                      /
                    </Kbd>
                  ) : undefined
                }
              />
            )}

            {filters && (
              <Popover
                opened={filterOpen}
                onChange={setFilterOpen}
                position="bottom-end"
                width={320}
                shadow="md"
                withArrow
              >
                <Popover.Target>
                  <Button
                    variant="default"
                    leftSection={<IconFilter size={15} stroke={1.5} />}
                    onClick={() => setFilterOpen((open) => !open)}
                    rightSection={
                      filterCount > 0 ? (
                        <Box
                          component="span"
                          style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            minWidth: 18,
                            height: 18,
                            padding: '0 5px',
                            fontSize: 11,
                            fontWeight: 500,
                            color: 'var(--mantine-color-white)',
                            background: 'var(--mantine-primary-color-filled)',
                          }}
                        >
                          {filterCount}
                        </Box>
                      ) : undefined
                    }
                  >
                    {t('list.filter')}
                  </Button>
                </Popover.Target>
                <Popover.Dropdown p={0}>
                  <Group justify="space-between" px="md" py="sm" style={{ borderBottom: HAIRLINE }}>
                    <Text ff="heading" fw={500} fz={14}>
                      {t('list.filterTitle')}
                    </Text>
                    <ActionIcon
                      variant="subtle"
                      color="gray"
                      aria-label={t('list.applyFilters')}
                      onClick={() => setFilterOpen(false)}
                    >
                      <IconX size={16} stroke={1.5} />
                    </ActionIcon>
                  </Group>
                  <Stack gap="sm" p="md">
                    {filters}
                  </Stack>
                  <Group justify="space-between" px="md" py="sm" style={{ borderTop: HAIRLINE }}>
                    <Button
                      variant="subtle"
                      color="gray"
                      onClick={onClearFilters}
                      disabled={!onClearFilters || filterCount === 0}
                    >
                      {t('list.clearFilters')}
                    </Button>
                    <Button onClick={() => setFilterOpen(false)}>{t('list.applyFilters')}</Button>
                  </Group>
                </Popover.Dropdown>
              </Popover>
            )}

            {onExport && (
              <Button
                variant="default"
                leftSection={<IconDownload size={15} stroke={1.5} />}
                onClick={onExport}
              >
                {t('list.export')}
              </Button>
            )}

            {primaryAction}
          </Group>
          </Group>
        </Box>

        {(tabs || pagination) && (
          <Group
            justify="space-between"
            align="stretch"
            gap="md"
            wrap="nowrap"
            px="md"
            style={{ minHeight: SUBBAR_HEIGHT, borderBottom: HAIRLINE }}
          >
            {tabs && tabs.length > 0 ? (
              <Group gap="lg" align="stretch" wrap="nowrap">
                {tabs.map((tab, index) => {
                  const active = index === activeTab
                  return (
                    <UnstyledButton
                      key={index}
                      onClick={() => onTabChange?.(index)}
                      ff="heading"
                      fw={500}
                      fz={13}
                      c={active ? 'var(--mantine-primary-color-filled)' : 'dimmed'}
                      style={{
                        boxShadow: active ? 'inset 0 -2px 0 var(--mantine-primary-color-filled)' : undefined,
                        alignSelf: 'stretch',
                        display: 'flex',
                        alignItems: 'center',
                      }}
                    >
                      {tab.label}
                      {tab.count !== undefined && (
                        <Text component="span" c="dimmed" fw={400} fz={13} ml={6}>
                          {tab.count}
                        </Text>
                      )}
                    </UnstyledButton>
                  )
                })}
              </Group>
            ) : (
              <span />
            )}
            {pagination}
          </Group>
        )}
      </Box>

      <Box
        style={{
          background: 'var(--mantine-color-white)',
          borderLeft: HAIRLINE,
          borderRight: HAIRLINE,
          borderBottom: HAIRLINE,
        }}
      >
        {children}
      </Box>
    </Box>
  )
}
