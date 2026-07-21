import { createElement, type ReactNode } from 'react'
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
  const markGlyph = filled ? markIcon(statusKey) : outlineIcon(statusKey)
  const iconSize = typeof size === 'number' ? Math.round(size * 0.55) : 10

  const tooltipGlyph = outlineIcon(statusKey)
  const mark = (
    <ThemeIcon
      size={size}
      radius="xl"
      color={color as MantineColor}
      variant={filled ? 'filled' : 'light'}
      aria-label={label}
    >
      {createElement(markGlyph, { size: iconSize, stroke: filled ? 2.5 : 1.75 })}
    </ThemeIcon>
  )

  if (!withTooltip) {
    return mark
  }

  return (
    <Tooltip
      label={
        <Group gap={6} wrap="nowrap">
          {createElement(tooltipGlyph, { size: 13, stroke: 2 })}
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
  const leftGlyph = outlineIcon(statusKey)
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
      leftSection={createElement(leftGlyph, { size: 14, stroke: 1.75 })}
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

/** Monochrome status treatments from design 2a: `accent` (steel fill),
 * `outline` (steel hairline, no fill), `neutral` (grey fill). */
export type StatusTone = 'accent' | 'outline' | 'neutral'

const TONE_STYLES: Record<StatusTone, { backgroundColor: string; borderColor: string; color: string }> = {
  accent: {
    backgroundColor: 'var(--mantine-color-steel-1)',
    borderColor: 'var(--mantine-color-steel-3)',
    color: 'var(--mantine-color-steel-8)',
  },
  outline: {
    backgroundColor: 'transparent',
    borderColor: 'var(--mantine-color-steel-6)',
    color: 'var(--mantine-color-steel-7)',
  },
  neutral: {
    backgroundColor: 'var(--mantine-color-gray-1)',
    borderColor: 'var(--mantine-color-gray-3)',
    color: 'var(--mantine-color-gray-8)',
  },
}

export interface StatusTagProps {
  color?: string
  label: string
  /** Design-2a monochrome tone. Overrides `color` when set. */
  tone?: StatusTone
}

/** Industry-style status tag: a square, hairline-bordered tinted label — no
 * icon. With `tone`, uses the 2a monochrome palette; otherwise tints from the
 * status color's Mantine palette. */
export function StatusTag({ color = 'gray', label, tone }: StatusTagProps) {
  const style = tone
    ? TONE_STYLES[tone]
    : {
        backgroundColor: `var(--mantine-color-${color}-1)`,
        borderColor: `var(--mantine-color-${color}-3)`,
        color: `var(--mantine-color-${color}-8)`,
      }
  return (
    <Badge
      variant="light"
      color={color as MantineColor}
      radius={0}
      size="md"
      tt="none"
      fw={500}
      styles={{
        root: {
          border: `1px solid ${style.borderColor}`,
          backgroundColor: style.backgroundColor,
          color: style.color,
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
