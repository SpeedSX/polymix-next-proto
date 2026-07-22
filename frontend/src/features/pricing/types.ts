import { z } from 'zod'

import i18n from '../../lib/i18n'
import { MONEY_DECIMAL_PATTERN } from '../../lib/money'

// A stored catalog document is the engine's own shape as JSON; the id is a
// full record id (`format:a5`). CRUD carries these verbatim.
export type CatalogDoc = Record<string, unknown> & { id?: string }

export const PRICING_ENTITIES = ['formats', 'materials', 'machines', 'operations', 'policies'] as const
export type PricingEntitySegment = (typeof PRICING_ENTITIES)[number]

// --- Money / unit conversions ------------------------------------------------
// The catalog stores money as micro-units (1_000_000 per currency unit),
// margins as basis points (17000 = ×1.7), and rounding step / floor as minor
// units (cents). Forms edit human decimals and multipliers; convert at the edge.

function toDecimalNumber(input: string): number {
  return Number.parseFloat(input.trim().replace(',', '.') || '0')
}

/** Drop trailing zeros without scientific notation, e.g. 41000µ → "0.041". */
function trimDecimal(value: number): string {
  return String(Number(value.toFixed(6)))
}

export function toMicro(decimal: string): number {
  return Math.round(toDecimalNumber(decimal) * 1_000_000)
}

export function fromMicro(micro: number): string {
  return trimDecimal(micro / 1_000_000)
}

export function toMinor(decimal: string): number {
  return Math.round(toDecimalNumber(decimal) * 100)
}

export function fromMinor(minor: number): string {
  return (minor / 100).toFixed(2)
}

export function multiplierToBp(multiplier: string): number {
  return Math.round(toDecimalNumber(multiplier) * 10_000)
}

export function bpToMultiplier(bp: number): string {
  return trimDecimal(bp / 10_000)
}

function tv(code: string): string {
  return i18n.t(`common:validation.${code}`)
}

// --- Format (12f) ------------------------------------------------------------

export const formatFormSchema = z
  .object({
    name: z.string().trim().min(1),
    width: z.coerce.number().int().min(1),
    height: z.coerce.number().int().min(1),
  })
  .superRefine((values, ctx) => {
    if (values.width > 0 && values.height > 0 && values.width > values.height) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['width'], message: tv('portrait_required') })
    }
  })

export type FormatFormValues = z.infer<typeof formatFormSchema>

export const emptyFormatFormValues: FormatFormValues = { name: '', width: 0, height: 0 }

export function toFormatDoc(values: FormatFormValues): CatalogDoc {
  return { name: values.name.trim(), trim_mm: [values.width, values.height] }
}

export function fromFormatDoc(doc: CatalogDoc): FormatFormValues {
  const trim = (doc.trim_mm as [number, number] | undefined) ?? [0, 0]
  return { name: String(doc.name ?? ''), width: trim[0], height: trim[1] }
}

// --- Material (12b) ----------------------------------------------------------

export const MATERIAL_BASES = ['per_sheet', 'per_m2', 'per_cm', 'per_item'] as const
export type MaterialBasis = (typeof MATERIAL_BASES)[number]

const attrRowSchema = z.object({ key: z.string(), value: z.string() })
export type AttrRow = z.infer<typeof attrRowSchema>

export const materialFormSchema = z
  .object({
    name: z.string().trim().min(1),
    kind: z.string().trim().min(1),
    basis: z.enum(MATERIAL_BASES),
    sheetWidth: z.coerce.number().int().min(0),
    sheetHeight: z.coerce.number().int().min(0),
    price: z.string(),
    printable: z.boolean(),
    grammage: z.coerce.number().int().min(0),
    attrs: z.array(attrRowSchema),
  })
  .superRefine((values, ctx) => {
    if (!MONEY_DECIMAL_PATTERN.test(values.price)) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['price'], message: tv('invalid_amount') })
    }
    if (values.basis === 'per_sheet') {
      if (values.sheetWidth < 1) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['sheetWidth'], message: tv('positive_dimensions') })
      }
      if (values.sheetHeight < 1) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['sheetHeight'], message: tv('positive_dimensions') })
      }
    }
    if (values.printable && values.grammage < 1) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['grammage'], message: tv('positive_grammage') })
    }
  })

export type MaterialFormValues = z.infer<typeof materialFormSchema>

export const emptyMaterialFormValues: MaterialFormValues = {
  name: '',
  kind: '',
  basis: 'per_sheet',
  sheetWidth: 320,
  sheetHeight: 450,
  price: '',
  printable: false,
  grammage: 0,
  attrs: [],
}

export function toMaterialDoc(values: MaterialFormValues): CatalogDoc {
  const priceMicro = toMicro(values.price)
  const pricing =
    values.basis === 'per_sheet'
      ? { basis: 'per_sheet', sheet_size_mm: [values.sheetWidth, values.sheetHeight], price_micro: priceMicro }
      : { basis: values.basis, price_micro: priceMicro }
  const attrs: Record<string, string> = {}
  for (const row of values.attrs) {
    const key = row.key.trim()
    if (key) attrs[key] = row.value
  }
  const doc: CatalogDoc = { name: values.name.trim(), kind: values.kind.trim(), pricing, attrs }
  if (values.printable) doc.printable = { grammage_gsm: values.grammage }
  return doc
}

export function fromMaterialDoc(doc: CatalogDoc): MaterialFormValues {
  const pricing = (doc.pricing ?? {}) as { basis?: MaterialBasis; sheet_size_mm?: [number, number]; price_micro?: number }
  const printable = doc.printable as { grammage_gsm: number } | undefined
  const attrs = (doc.attrs ?? {}) as Record<string, string>
  const sheet = pricing.sheet_size_mm ?? [320, 450]
  return {
    name: String(doc.name ?? ''),
    kind: String(doc.kind ?? ''),
    basis: pricing.basis ?? 'per_sheet',
    sheetWidth: sheet[0],
    sheetHeight: sheet[1],
    price: pricing.price_micro !== undefined ? fromMicro(pricing.price_micro) : '',
    printable: printable !== undefined,
    grammage: printable?.grammage_gsm ?? 0,
    attrs: Object.entries(attrs).map(([key, value]) => ({ key, value: String(value) })),
  }
}

// --- Machine (12c) -----------------------------------------------------------

export const MACHINE_TECHNOLOGIES = ['digital', 'offset'] as const
export type MachineTechnology = (typeof MACHINE_TECHNOLOGIES)[number]

export const machineFormSchema = z
  .object({
    name: z.string().trim().min(1),
    technology: z.enum(MACHINE_TECHNOLOGIES),
    sheetWidth: z.coerce.number().int().min(1),
    sheetHeight: z.coerce.number().int().min(1),
    maxGrammage: z.coerce.number().int().min(1),
    duplex: z.boolean(),
    setup: z.string(),
    wasteFixedSheets: z.coerce.number().int().min(0),
    wastePercent: z.coerce.number().int().min(0).max(100),
    clickMono: z.string(),
    clickColor: z.string(),
    platePrice: z.string(),
    runPrice: z.string(),
  })
  .superRefine((values, ctx) => {
    if (!MONEY_DECIMAL_PATTERN.test(values.setup)) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['setup'], message: tv('invalid_amount') })
    }
    if (values.technology === 'digital') {
      for (const field of ['clickMono', 'clickColor'] as const) {
        if (toMicro(values[field]) <= 0) {
          ctx.addIssue({ code: z.ZodIssueCode.custom, path: [field], message: tv('required_for_digital') })
        }
      }
    } else {
      for (const field of ['platePrice', 'runPrice'] as const) {
        if (toMicro(values[field]) <= 0) {
          ctx.addIssue({ code: z.ZodIssueCode.custom, path: [field], message: tv('required_for_offset') })
        }
      }
    }
  })

export type MachineFormValues = z.infer<typeof machineFormSchema>

export const emptyMachineFormValues: MachineFormValues = {
  name: '',
  technology: 'digital',
  sheetWidth: 320,
  sheetHeight: 450,
  maxGrammage: 350,
  duplex: true,
  setup: '',
  wasteFixedSheets: 0,
  wastePercent: 0,
  clickMono: '',
  clickColor: '',
  platePrice: '',
  runPrice: '',
}

export function toMachineDoc(values: MachineFormValues): CatalogDoc {
  const digital = values.technology === 'digital'
  return {
    name: values.name.trim(),
    technology: values.technology,
    sheet_size_mm: [values.sheetWidth, values.sheetHeight],
    duplex: values.duplex,
    max_grammage_gsm: values.maxGrammage,
    setup_micro: toMicro(values.setup),
    waste_fixed_sheets: values.wasteFixedSheets,
    waste_percent: values.wastePercent,
    // Only the technology's own cost pair is populated; the other stays 0 so
    // the server's mutual-exclusion check (not_for_digital / not_for_offset)
    // can never fire.
    click_mono_micro: digital ? toMicro(values.clickMono) : 0,
    click_color_micro: digital ? toMicro(values.clickColor) : 0,
    plate_price_micro: digital ? 0 : toMicro(values.platePrice),
    run_price_micro: digital ? 0 : toMicro(values.runPrice),
  }
}

export function fromMachineDoc(doc: CatalogDoc): MachineFormValues {
  const sheet = (doc.sheet_size_mm as [number, number] | undefined) ?? [320, 450]
  const micro = (key: string) => fromMicro(Number(doc[key] ?? 0))
  const positive = (key: string) => (Number(doc[key] ?? 0) > 0 ? micro(key) : '')
  return {
    name: String(doc.name ?? ''),
    technology: (doc.technology as MachineTechnology) ?? 'digital',
    sheetWidth: sheet[0],
    sheetHeight: sheet[1],
    maxGrammage: Number(doc.max_grammage_gsm ?? 0),
    duplex: Boolean(doc.duplex),
    setup: micro('setup_micro'),
    wasteFixedSheets: Number(doc.waste_fixed_sheets ?? 0),
    wastePercent: Number(doc.waste_percent ?? 0),
    clickMono: positive('click_mono_micro'),
    clickColor: positive('click_color_micro'),
    platePrice: positive('plate_price_micro'),
    runPrice: positive('run_price_micro'),
  }
}

// --- Operation (12d) ---------------------------------------------------------

export const UNIT_BASES = ['per_item', 'per_sheet', 'per_cm', 'per_m2'] as const
export type UnitBasis = (typeof UNIT_BASES)[number]

export const operationFormSchema = z
  .object({
    name: z.string().trim().min(1),
    unitBasis: z.enum(UNIT_BASES),
    setup: z.string(),
    unitPrice: z.string(),
  })
  .superRefine((values, ctx) => {
    for (const field of ['setup', 'unitPrice'] as const) {
      if (!MONEY_DECIMAL_PATTERN.test(values[field])) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: [field], message: tv('invalid_amount') })
      }
    }
  })

export type OperationFormValues = z.infer<typeof operationFormSchema>

export const emptyOperationFormValues: OperationFormValues = {
  name: '',
  unitBasis: 'per_item',
  setup: '',
  unitPrice: '',
}

export function toOperationDoc(values: OperationFormValues): CatalogDoc {
  return {
    name: values.name.trim(),
    unit_basis: values.unitBasis,
    setup_micro: toMicro(values.setup),
    unit_price_micro: toMicro(values.unitPrice),
  }
}

export function fromOperationDoc(doc: CatalogDoc): OperationFormValues {
  return {
    name: String(doc.name ?? ''),
    unitBasis: (doc.unit_basis as UnitBasis) ?? 'per_item',
    setup: fromMicro(Number(doc.setup_micro ?? 0)),
    unitPrice: fromMicro(Number(doc.unit_price_micro ?? 0)),
  }
}

// --- Pricing policy (12e) ----------------------------------------------------
// Note: spec §2 has no `name` on pricing_policy; policies are labelled by
// currency. `rounding.mode` is pinned to "up".

const bandRowSchema = z.object({ minQty: z.coerce.number().int().min(1), multiplier: z.string() })
export type BandRow = z.infer<typeof bandRowSchema>

export const CURRENCY_OPTIONS = ['EUR', 'GBP', 'USD', 'UAH'] as const

export const policyFormSchema = z
  .object({
    currency: z.string().length(3),
    bands: z.array(bandRowSchema),
    roundingStep: z.string(),
    minPrice: z.string(),
  })
  .superRefine((values, ctx) => {
    if (!MONEY_DECIMAL_PATTERN.test(values.roundingStep) || toMinor(values.roundingStep) <= 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['roundingStep'], message: tv('positive_step') })
    }
    if (!MONEY_DECIMAL_PATTERN.test(values.minPrice)) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['minPrice'], message: tv('invalid_amount') })
    }
    if (values.bands.length === 0) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['bands'], message: tv('bands_required') })
      return
    }
    if (values.bands[0].minQty !== 1) {
      ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['bands', 0, 'minQty'], message: tv('first_band_min_qty_one') })
    }
    values.bands.forEach((band, index) => {
      if (multiplierToBp(band.multiplier) <= 0) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['bands', index, 'multiplier'], message: tv('positive_multiplier') })
      }
      if (index > 0 && band.minQty <= values.bands[index - 1].minQty) {
        ctx.addIssue({ code: z.ZodIssueCode.custom, path: ['bands', index, 'minQty'], message: tv('bands_ascending') })
      }
    })
  })

export type PolicyFormValues = z.infer<typeof policyFormSchema>

export function emptyPolicyFormValues(currency = 'EUR'): PolicyFormValues {
  return {
    currency,
    bands: [{ minQty: 1, multiplier: '1.7' }],
    roundingStep: '0.10',
    minPrice: '25.00',
  }
}

export function toPolicyDoc(values: PolicyFormValues): CatalogDoc {
  return {
    currency: values.currency.toUpperCase(),
    margin_bands: values.bands.map((band) => ({
      min_qty: band.minQty,
      multiplier_bp: multiplierToBp(band.multiplier),
    })),
    rounding: { step_minor: toMinor(values.roundingStep), mode: 'up' },
    min_price_minor: toMinor(values.minPrice),
  }
}

export function fromPolicyDoc(doc: CatalogDoc): PolicyFormValues {
  const bands = (doc.margin_bands ?? []) as { min_qty: number; multiplier_bp: number }[]
  const rounding = (doc.rounding ?? { step_minor: 10 }) as { step_minor: number }
  return {
    currency: String(doc.currency ?? 'EUR'),
    bands: bands.map((band) => ({ minQty: band.min_qty, multiplier: bpToMultiplier(band.multiplier_bp) })),
    roundingStep: fromMinor(rounding.step_minor),
    minPrice: fromMinor(Number(doc.min_price_minor ?? 0)),
  }
}

// --- API validation-error field mapping --------------------------------------
// Backend field key → form path across all entities (the key sets are
// disjoint per entity). `_` marks a whole-document shape error the form
// surfaces as a form-level message.

const FIELD_MAP: Record<string, string> = {
  _: '',
  trim_mm: 'width',
  pricing: 'price',
  printable: 'grammage',
  click_mono_micro: 'clickMono',
  click_color_micro: 'clickColor',
  plate_price_micro: 'platePrice',
  run_price_micro: 'runPrice',
  setup_micro: 'setup',
  unit_price_micro: 'unitPrice',
  margin_bands: 'bands',
  rounding: 'roundingStep',
  min_price_minor: 'minPrice',
}

export function mapApiErrorField(field: string): string {
  return FIELD_MAP[field] ?? field
}
