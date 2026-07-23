import type { Column } from '@tanstack/react-table'
import { describe, expect, it } from 'vitest'

import { columnAlign, columnWidth } from './table'

function column(meta?: { align?: 'right'; width?: number }): Column<unknown, unknown> {
  return { columnDef: { meta } } as unknown as Column<unknown, unknown>
}

describe('table column meta helpers', () => {
  it('reads the alignment from column meta', () => {
    expect(columnAlign(column({ align: 'right' }))).toBe('right')
  })

  it('returns undefined alignment when meta is absent or unset', () => {
    expect(columnAlign(column())).toBeUndefined()
    expect(columnAlign(column({ width: 80 }))).toBeUndefined()
  })

  it('reads the fixed width from column meta', () => {
    expect(columnWidth(column({ width: 120 }))).toBe(120)
  })

  it('returns undefined width when meta is absent or unset', () => {
    expect(columnWidth(column())).toBeUndefined()
    expect(columnWidth(column({ align: 'right' }))).toBeUndefined()
  })
})
