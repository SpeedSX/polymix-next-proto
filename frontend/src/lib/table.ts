import type { Column } from '@tanstack/react-table'

declare module '@tanstack/react-table' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData, TValue> {
    align?: 'right'
  }
}

/** Text alignment from column meta (numeric columns use `align: 'right'`). */
export function columnAlign<TData>(column: Column<TData, unknown>): 'right' | undefined {
  return column.columnDef.meta?.align
}
