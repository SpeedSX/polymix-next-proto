import { TextInput } from '@mantine/core'
import type { TextInputProps } from '@mantine/core'

/**
 * Decimal money input for the catalog's micro-unit fields. Holds a plain
 * decimal string bound via `form.getInputProps`; the owning form converts
 * to/from micro-units with `toMicro`/`fromMicro` at the edge, so money never
 * round-trips through a float in component state.
 */
export function MoneyMicroInput({ currency, ...props }: TextInputProps & { currency?: string }) {
  return <TextInput inputMode="decimal" rightSection={currency ? <span style={{ fontSize: 12, color: 'var(--mantine-color-dimmed)' }}>{currency}</span> : undefined} {...props} />
}
