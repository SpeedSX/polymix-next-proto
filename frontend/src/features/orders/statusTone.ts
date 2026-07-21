import type { StatusTone } from '../../components/StatusBadge'

// Design 2a maps order stages onto the monochrome steel/grey palette rather
// than semantic hues: active production reads as steel, terminal states recede
// to grey, and the confirmed hand-off is the outline step between them.
const ORDER_STATUS_TONES: Record<string, StatusTone> = {
  draft: 'neutral',
  confirmed: 'outline',
  in_production: 'accent',
  completed: 'accent',
  cancelled: 'neutral',
}

export function orderStatusTone(statusKey: string | undefined): StatusTone {
  return (statusKey && ORDER_STATUS_TONES[statusKey]) || 'neutral'
}
