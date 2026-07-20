import type { ReactNode } from 'react'
import { Badge, Group, ThemeIcon, Tooltip } from '@mantine/core'
import type { ComboboxItem, ComboboxLikeRenderOptionInput, MantineColor, MantineSize } from '@mantine/core'
import {
  IconBan,
  IconCheck,
  IconCircleCheck,
  IconCircleDashed,
  IconClock,
  IconFileText,
  IconPlayerPause,
  IconUserPlus,
  IconX,
  type Icon,
} from '@tabler/icons-react'

const STATUS_ICONS: Record<string, Icon> = {
  draft: IconFileText,
  confirmed: IconCircleCheck,
  in_production: IconClock,
  completed: IconCircleCheck,
  cancelled: IconX,
  lead: IconUserPlus,
  active: IconCircleCheck,
  inactive: IconPlayerPause,
  blocked: IconBan,
}

/** Filled/inverted mark glyphs (used for selected filter options). */
const STATUS_MARKS: Record<string, Icon> = {
  draft: IconCircleDashed,
  confirmed: IconCheck,
  in_production: IconClock,
  completed: IconCheck,
  cancelled: IconX,
  lead: IconCheck,
  active: IconCheck,
  inactive: IconPlayerPause,
  blocked: IconBan,
}

function outlineIcon(statusKey?: string): Icon {
  return (statusKey && STATUS_ICONS[statusKey]) || IconCircleDashed
}

function markIcon(statusKey?: string): Icon {
  return (statusKey && STATUS_MARKS[statusKey]) || IconCheck
}

export interface StatusMarkProps {
  statusKey?: string
  color?: string
  label: string
  size?: MantineSize | number
  withTooltip?: boolean
  /** `light` = soft outline (lists); `filled` = bright inverted (selected filter item). */
  variant?: 'light' | 'filled'
}

export function StatusMark({
  statusKey,
  color = 'gray',
  label,
  size = 22,
  withTooltip = true,
  variant = 'light',
}: StatusMarkProps) {
  const filled = variant === 'filled'
  const MarkIcon = filled ? markIcon(statusKey) : outlineIcon(statusKey)
  const iconSize = typeof size === 'number' ? Math.round(size * 0.55) : 10

  const TooltipIcon = outlineIcon(statusKey)
  const mark = (
    <ThemeIcon
      size={size}
      radius="xl"
      color={color as MantineColor}
      variant={filled ? 'filled' : 'light'}
      aria-label={label}
    >
      <MarkIcon size={iconSize} stroke={filled ? 2.5 : 1.75} />
    </ThemeIcon>
  )

  if (!withTooltip) {
    return mark
  }

  return (
    <Tooltip
      label={
        <Group gap={6} wrap="nowrap">
          <TooltipIcon size={13} stroke={2} />
          <span>{label}</span>
        </Group>
      }
      color={color as MantineColor}
      radius="md"
      withArrow
      arrowSize={6}
      offset={6}
      openDelay={150}
      transitionProps={{ transition: 'pop', duration: 150 }}
      styles={{ tooltip: { fontWeight: 600, letterSpacing: '0.01em', padding: '5px 10px' } }}
    >
      {mark}
    </Tooltip>
  )
}

export interface StatusBadgeProps {
  statusKey?: string
  color?: string
  label: string
  count?: number
}

export function StatusBadge({ statusKey, color = 'gray', label, count }: StatusBadgeProps) {
  const LeftIcon = outlineIcon(statusKey)
  const mantineColor = color as MantineColor

  return (
    <Badge
      color={mantineColor}
      variant="light"
      radius="xl"
      size="lg"
      tt="none"
      fw={500}
      px="sm"
      leftSection={<LeftIcon size={14} stroke={1.75} />}
      styles={{
        root: {
          border: `1px solid var(--mantine-color-${color}-3)`,
          gap: 6,
        },
        label: {
          display: 'inline-flex',
          alignItems: 'center',
          gap: 6,
        },
      }}
    >
      {label}
      {count != null && <span>{count}</span>}
    </Badge>
  )
}

export interface StatusTagProps {
  color?: string
  label: string
}

/** Industry-style status tag: a square, hairline-bordered tinted label — no
 * icon. Reads its tint from the status color's Mantine palette. */
export function StatusTag({ color = 'gray', label }: StatusTagProps) {
  return (
    <Badge
      variant="light"
      color={color as MantineColor}
      radius={0}
      size="md"
      tt="none"
      fw={600}
      styles={{
        root: {
          border: `1px solid var(--mantine-color-${color}-3)`,
          backgroundColor: `var(--mantine-color-${color}-1)`,
          color: `var(--mantine-color-${color}-8)`,
          letterSpacing: '0.02em',
        },
      }}
    >
      {label}
    </Badge>
  )
}

export interface StatusOptionMeta {
  key?: string
  color?: string
}

/** Resolve dictionary metadata for a Select option value (status id as string). */
export function statusMetaFor(
  byId: Map<number, { key: string; color: string }>,
  value: string | null | undefined,
): StatusOptionMeta {
  if (value == null) return {}
  const item = byId.get(Number(value))
  return { key: item?.key, color: item?.color }
}

export function renderStatusSelectOption(
  byId: Map<number, { key: string; color: string }>,
): (input: ComboboxLikeRenderOptionInput<ComboboxItem>) => ReactNode {
  return ({ option, checked }) => {
    const meta = statusMetaFor(byId, option.value)
    return (
      <Group gap="xs" wrap="nowrap">
        <StatusMark
          statusKey={meta.key}
          color={meta.color}
          label={option.label}
          size={18}
          withTooltip={false}
          variant={checked ? 'filled' : 'light'}
        />
        <span>{option.label}</span>
      </Group>
    )
  }
}
