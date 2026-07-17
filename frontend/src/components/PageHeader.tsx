import type { ReactNode } from 'react'
import { Box, Group, Text, Title } from '@mantine/core'

export interface PageHeaderProps {
  /** Trail of crumbs, rendered uppercase and slash-separated (e.g. ['Customers', 'Edit']). */
  breadcrumb: string[]
  title: ReactNode
  /** Inline slot beside the title — typically a status tag. */
  status?: ReactNode
  /** Right-aligned action buttons. */
  actions?: ReactNode
  /** Pin to the top of the scroll area while the body scrolls under it. */
  sticky?: boolean
}

export function PageHeader({ breadcrumb, title, status, actions, sticky }: PageHeaderProps) {
  return (
    <Box
      style={{
        position: sticky ? 'sticky' : undefined,
        top: sticky ? 0 : undefined,
        zIndex: sticky ? 5 : undefined,
        background: 'var(--mantine-color-body)',
        borderBottom: '1px solid var(--mantine-color-gray-3)',
        paddingTop: sticky ? 8 : undefined,
        paddingBottom: 12,
      }}
    >
      <Group justify="space-between" align="flex-end" wrap="nowrap" gap="md">
        <div style={{ minWidth: 0 }}>
          <Text fz={11} fw={500} tt="uppercase" c="dimmed" style={{ letterSpacing: '0.08em' }}>
            {breadcrumb.map((crumb, index) => (
              <span key={index}>
                {index > 0 && <span style={{ opacity: 0.5, margin: '0 6px' }}>/</span>}
                {crumb}
              </span>
            ))}
          </Text>
          <Group gap={10} align="center" wrap="nowrap" mt={3}>
            <Title order={2} fz={24} lh={1.1} style={{ margin: 0, minWidth: 0 }}>
              {title}
            </Title>
            {status}
          </Group>
        </div>
        {actions && (
          <Group gap="sm" wrap="nowrap" style={{ flex: 'none' }}>
            {actions}
          </Group>
        )}
      </Group>
    </Box>
  )
}
