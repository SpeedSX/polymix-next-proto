# Quote Engine — Implementation Specification

Normative spec for `crates/quote-engine` and the pricing data model. Written to be implemented mechanically: every schema, formula, and expected number is stated exactly. Where this document and the narrative docs (`instant-quote.md`, `product-configuration.md`) disagree, **this document wins**.

Implementation checklist (suggested module layout):

```
crates/quote-engine/src/
  money.rs      §1  integer money helpers
  model.rs      §2  PriceModel structs (formats, materials, machines, operations, policy)
  effect.rs     §3  Effect enum + ColorsSpec
  resolve.rs    §4  selection → JobSpec
  rules.rs      §5  compatibility-rule AST + evaluation
  price.rs      §6  JobSpec → Breakdown → Ladder
  fixtures/     §9  golden dataset + assertions
```

## §1 Conventions

- **Money**: all internal amounts are `i64` in **micro-units** (µ) of the tenant currency: `1 EUR = 1_000_000 µ`; `1 minor unit (cent) = 10_000 µ`. No floats anywhere in the engine.
- **Lengths**: `u32` millimetres. **Areas**: `u64` mm². Formats are stored portrait: `trim_mm = [width, height]` with `width <= height`.
- **Integer helpers** (all operands positive):
  - `ceil_div(a, b) = (a + b - 1) / b`
  - `round_half_up(a, b) = (a + b / 2) / b`  — divide `a` by `b`, rounding .5 up
- **Determinism**: same `PriceModel` + same request ⇒ byte-identical response. No clock, no randomness.
- **Engine constant**: `BLEED_MM: u32 = 3` (per side). v1 has no per-tenant override.
- **IDs**: SurrealDB record ids as strings — `format:a5`, `material:offset_80`, `machine:digi1`, `operation:cutting`, `pricing_policy:standard`.

## §2 PriceModel — data schema

All tables are per-tenant. Field names below are normative for both the Rust structs and the stored JSON.

### `format`

| Field | Type | Notes |
|---|---|---|
| `id` | record id | |
| `name` | string | display name ("A5", "DL") |
| `trim_mm` | `[u32, u32]` | `[width, height]`, portrait (`width <= height`) |

### `material`

One catalog table for every material — papers, boards, films, foils, spiral wire, boxes. Factored by **role in the engine contract**, not by material family: the engine reads `pricing` (how the material is consumed) and `printable` (whether a press can print on it); everything else is opaque metadata.

| Field | Type | Notes |
|---|---|---|
| `id`, `name` | | |
| `kind` | string | UI classification only (`"paper"`, `"film"`, `"wire"`, `"foil"`, …) — free taxonomy, the engine never branches on it |
| `pricing` | `MaterialPricing` | tagged union, see below |
| `printable` | `{ grammage_gsm: u32 }` or absent | present only for substrates a press can print on; read by machine capability checks |
| `attrs` | map | opaque descriptive metadata (colour, brand, supplier, thickness) for admin UI and production docs |

`MaterialPricing`, serde `#[serde(tag = "basis", rename_all = "snake_case")]`:

```rust
enum MaterialPricing {
    PerSheet { sheet_size_mm: [u32; 2], price_micro: i64 },
    PerM2    { price_micro: i64 },
    PerCm    { price_micro: i64 },
    PerItem  { price_micro: i64 },
}
```

Engine-enforced constraints:
- A component's `material` must have `per_sheet` pricing (`E107`, checked at resolution — imposition needs a sheet size).
- Printing on a component additionally requires `printable`: a material without it (or over a machine's grammage cap) yields no capable machine (`E201`).
- An operation's `material` param must have a pricing basis equal to the operation's `unit_basis` (`E204`).

A new material *family* is a new `kind` string plus `attrs` — pure data, no deploy. A new *consumption model* (e.g. `per_kg` for ink) is a new `MaterialPricing` variant — an engine release, consistent with the closed-vocabulary rule.

### `machine`

| Field | Type | Notes |
|---|---|---|
| `id`, `name` | | |
| `technology` | `"digital" \| "offset"` | |
| `sheet_size_mm` | `[u32, u32]` | press-sheet size |
| `duplex` | bool | can print the back side |
| `max_grammage_gsm` | `u32` | |
| `setup_micro` | `i64` | fixed per component run |
| `waste_fixed_sheets` | `u32` | added after percentage waste |
| `waste_percent` | `u32` | whole percent (2 = 2 %) |
| `click_mono_micro` | `i64` | **digital only** — one mono side of one sheet |
| `click_color_micro` | `i64` | **digital only** — one colour side of one sheet |
| `plate_price_micro` | `i64` | **offset only** — per plate |
| `run_price_micro` | `i64` | **offset only** — per sheet through the press |

v1 simplification (checked at pricing, error `E203` folded into capability): a machine can only print a material whose `per_sheet` `sheet_size_mm` equals the machine's `sheet_size_mm`.

### `operation`

| Field | Type | Notes |
|---|---|---|
| `id`, `name` | | |
| `setup_micro` | `i64` | fixed per job when the operation is present |
| `unit_basis` | `"per_item" \| "per_sheet" \| "per_cm" \| "per_m2"` | |
| `unit_price_micro` | `i64` | labour/machine share; material is added via the `material` param (§6.3) |

**Reserved operation params** (read by the engine; all other params are opaque and passed through to production):

| Param key | Value | Meaning |
|---|---|---|
| `material` | material record id | its pricing `price_micro` is added to the operation's unit price; pricing basis must equal `unit_basis` (`E204`) |
| `edge_mm` | `u32` | overrides the default edge for `per_cm` (default: `trim_mm[1]`, the height) |
| `units_multiplier` | `u32 >= 1`, default 1 | multiplies the unit term of the §6.3 formula (2 = double-sided lamination, 4 = four drill holes); `setup_micro` is deliberately **not** multiplied — one changeover regardless. Invalid value → `E205` |

### `pricing_policy`

```jsonc
{
  "id": "pricing_policy:standard",
  "currency": "EUR",
  "margin_bands": [                       // sorted by min_qty ascending; first band MUST have min_qty 1
    { "min_qty": 1,    "multiplier_bp": 17000 },   // basis points: 17000 = ×1.7
    { "min_qty": 250,  "multiplier_bp": 16000 },
    { "min_qty": 1000, "multiplier_bp": 15000 }
  ],
  "rounding": { "step_minor": 10, "mode": "up" },  // v1: mode is always "up"
  "min_price_minor": 2500
}
```

Band selection: the band with the **largest `min_qty` ≤ qty**.

### `product_template`

Stored as **one document** with parameters/options embedded (atomic edits, natural ordering). Formats, materials, machines, operations, policies, and rules are separate tables.

```jsonc
{
  "id": "product_template:spiral_notebook",
  "slug": "spiral-notebook",
  "name": { "en": "Spiral notebook" },
  "components": ["cover", "interior"],          // roles that exist before any effect runs
  "pricing_policy": "pricing_policy:standard",
  "quantities": [50, 100, 200, 300, 500, 1000, 2000],
  "custom_quantity": { "min": 50, "max": 5000 },  // null → custom qty not offered
  "base_effects": [ Effect, ... ],
  "parameters": [ Parameter, ... ]
}

Parameter (select):  { "code", "label": {i18n}, "kind": "select", "options": [Option, ...] }
Parameter (numeric): { "code", "label": {i18n}, "kind": "numeric",
                       "input": { "min": u32, "max": u32, "step": u32 },
                       "default": u32?,                          // used when the parameter is omitted (§4.1)
                       "effects": [ Effect, ... ] }              // may use $input (§4.3)
Option:              { "code", "label": {i18n}, "is_default": bool,
                       "available": bool,                        // default true; standing admin flag:
                                                                 // option temporarily not fulfillable (§5)
                       "unavailable_message": {i18n}?,           // portal tooltip when disabled for unavailability
                       "effects": [ Effect, ... ] }
```

Exactly one option per select parameter may have `is_default: true`. A numeric `default` must satisfy its own `input` constraints (`min <= default <= max`, `(default - min) % step == 0`) — violating this is template error `E108`.

### `compatibility_rule` — see §5.

### `pricelist_version`

A single record `meta:pricing { version: int }` per tenant database. Every admin mutation of any table in this section increments it (application code, same transaction). The in-memory `PriceModel` snapshot carries the version it was built from; quote responses echo it.

## §3 Effect — normative serialization

Serde: `#[serde(tag = "kind", rename_all = "snake_case")]`. This enum is **closed**: an unknown `kind` fails template deserialization.

```rust
enum Effect {
    SetFormat        { format: String },                       // format record id
    SetPages         { target: String, value: NumOrInput },    // logical pages, see §4.3
    SetColors        { target: String, value: ColorsSpec },
    SetMaterial      { target: String, material: String },     // material id (per_sheet-priced)
    AddOperation     { operation: String,
                       #[serde(default)] params: Map<String, Json> },
    SetOpParam       { operation: String, param: String, value: Json },
    AddComponent     { role: String, pages: u32, colors: ColorsSpec, material: String },
    ConstrainTechnology { allow: Vec<Technology> },             // "digital" | "offset"
}
```

**`ColorsSpec`**: serialized as the string `"F/B"` where `F`,`B` are integers `0..=8` (`"4/0"`, `"4/4"`, `"1/1"`, `"0/0"`). Parse with `^([0-8])/([0-8])$` into `Colors { front: u8, back: u8 }`. Any other shape is a deserialization error.

JSON examples (canonical — the narrative docs' templates use these exact field names):

```json
{ "kind": "set_format",  "format": "format:a5" }
{ "kind": "set_pages",   "target": "interior", "value": 100 }
{ "kind": "set_colors",  "target": "cover", "value": "4/0" }
{ "kind": "set_material", "target": "cover", "material": "material:gloss_300" }
{ "kind": "add_operation", "operation": "operation:lamination",
  "params": { "material": "material:film_gloss" } }
{ "kind": "set_op_param", "operation": "operation:spiral_binding",
  "param": "material", "value": "material:spiral_black" }
{ "kind": "add_component", "role": "backing", "pages": 2, "colors": "0/0",
  "material": "material:board_500" }
{ "kind": "constrain_technology", "allow": ["digital"] }
```

## §4 Resolution — selection → JobSpec

### §4.1 Selection payload

A JSON object mapping parameter `code` → chosen value: option `code` (string) for select parameters, number for numeric parameters.

```json
{ "format": "a5", "printing": "4_0", "sheets": "50", "cover_stock": "gloss300",
  "interior_stock": "offset80", "spiral_color": "black", "lamination": "none",
  "backing": "yes" }
```

Validation, in order (first failure wins, error `INVALID_SELECTION` with the parameter code):
1. Every key must match a parameter code of the template; unknown keys are rejected.
2. A missing select parameter takes its `is_default` option; a missing numeric parameter takes its `default`. If the parameter has neither, reject.
3. A select value must match an option code of that parameter.
4. The chosen option — explicit or defaulted — must have `available: true`; otherwise reject with `reason: "option_unavailable"`. (The API must enforce this: the portal's greyed-out UI is not a security boundary.)
5. A numeric value must satisfy `min <= v <= max` and `(v - min) % step == 0`. (A defaulted value is guaranteed valid by `E108`.)

### §4.2 Algorithm

```text
resolve(template, selection, qty) -> Result<JobSpec, ResolutionError>

1. spec = JobSpec {
     format: None, quantity: qty,
     components: template.components.map(role -> Component {
         role, pages: None, colors: None, material: None }),
     operations: [], technology_allow: None }
2. apply template.base_effects, in array order
3. for each parameter in template.parameters (array order):
       select  -> apply the chosen option's effects, in array order
       numeric -> apply the parameter's effects with $input substituted (§4.3)
4. completeness check (E106):
       format is set; quantity >= 1;
       every component has pages >= 1, colors set, material set (with per_sheet pricing)
5. return spec
```

**Per-effect semantics.** "Target must exist" means the role is among `template.components` or was added by a previous `add_component`.

| Effect | Semantics | Error when |
|---|---|---|
| `set_format` | overwrite `spec.format` | referenced format id unknown → `E107` |
| `set_pages` | overwrite `component.pages` | target role absent → `E101` |
| `set_colors` | overwrite `component.colors` | target role absent → `E101` |
| `set_material` | overwrite `component.material` | target role absent → `E101`; material id unknown or not `per_sheet` priced → `E107` |
| `add_operation` | append `{operation, params}` | operation id already present in spec → `E102`; id unknown → `E107` |
| `set_op_param` | set `params[param] = value` on the matching operation instance | operation not present in spec (yet) → `E103` |
| `add_component` | append a fully-specified component | role already exists → `E104` |
| `constrain_technology` | `allow = allow ∩ previous` (first one just sets) | intersection empty → `E105` |

Later writes win; effect order is fully determined by step 2–3. Overwrites across *different* parameters are legal at runtime but flagged by lint (§8).

### §4.3 Numeric input substitution

Inside a numeric parameter's effects, any field of type `NumOrInput` accepts either a literal integer or:

```json
{ "$input": { "mul": 2, "add": 0 } }
```

Value = `input * mul + add` (defaults `mul: 1`, `add: 0`). Example — a "number of sheets" numeric parameter where 1 sheet = 2 pages:

```json
{ "code": "sheets", "kind": "numeric", "input": { "min": 20, "max": 200, "step": 10 },
  "effects": [ { "kind": "set_pages", "target": "interior",
                 "value": { "$input": { "mul": 2 } } } ] }
```

`$input` anywhere outside a numeric parameter's effects → template deserialization error `E110`.

## §5 Compatibility rules

Stored per template in table `compatibility_rule`. **No text DSL** — rules are a JSON AST.

```jsonc
{
  "id": "compatibility_rule:spiral_min_pages",
  "template": "product_template:spiral_notebook",
  "when":    { "op_present": "operation:spiral_binding" },   // optional; absent = always
  "require": { "attr": "component:interior.pages", "op": "gte", "value": 20 },
  "message": { "en": "Spiral binding requires at least 20 pages",
               "pl": "Bindowanie spiralne wymaga min. 20 stron" }
}
```

**Condition** (recursive):

```text
Condition =
  { "all": [Condition, ...] }        // AND
| { "any": [Condition, ...] }        // OR
| { "not": Condition }
| { "op_present": "<operation id>" }
| { "attr": Path, "op": "eq"|"ne"|"gte"|"lte"|"in", "value": Json }
```

**Path** (evaluated against the resolved `JobSpec`):

```text
"quantity" | "format"                          // format compares by record id
"component:<role>.pages"
"component:<role>.colors.front" | "component:<role>.colors.back"
"component:<role>.material"                    // compares by record id
```

Evaluation: an `attr` condition on a missing component (or unset attribute) evaluates to **false**. A rule is **violated** iff `when` evaluates true AND `require` evaluates false. On quote: all violated rules are returned together (`RULE_VIOLATION`, §7), not just the first.

**Option availability** (`GET /schema`): an option with `available: false` is marked `disabled: true` with `reason: "unavailable"` (plus its localized `unavailable_message`, if any) — a standing admin flag, independent of the current selection, no resolution needed. For every remaining option `o` of parameter `p`, re-run `resolve` with the current selection but `p := o` (numeric parameters: validate range only); mark `disabled: true` with `reason: "incompatible"` if resolution fails or any rule is violated. Cost: O(total options) resolutions per call — resolution is pure in-memory struct manipulation; this is well under a millisecond.

## §6 Pricing — JobSpec → Breakdown

### §6.1 Imposition

```text
ups(sheet: [w,h], trim: [w,h]) -> u32:
    fp = [trim.w + 2*BLEED_MM, trim.h + 2*BLEED_MM]
    a  = (sheet.w / fp.w) * (sheet.h / fp.h)      // integer division
    b  = (sheet.w / fp.h) * (sheet.h / fp.w)      // rotated
    max(a, b)                                     // 0 → E202 (item larger than sheet)
```

Reference values (sheet 320×450, bleed 3): A6 → 8, DL → 6, A5 → 4, A4 → 2. Ship this as a unit test.

### §6.2 Component costing

```text
leaves       = ceil_div(component.pages, 2)          // 1 leaf = 2 logical pages
raw_sheets   = ceil_div(qty * leaves, ups)
```

**Unprinted component** (`colors == 0/0`): no machine, no waste.
`ups` is computed against `material.pricing.sheet_size_mm`; `cost = raw_sheets * material.pricing.price_micro`; `sheets = raw_sheets`.

**Printed component**: evaluate every machine, keep the cheapest.

A machine is **capable** iff all of:
- `technology_allow` is `None` or contains `machine.technology`
- `colors.back > 0` implies `machine.duplex`
- `material.printable` is present, and `printable.grammage_gsm <= machine.max_grammage_gsm`
- `material.pricing.sheet_size_mm == machine.sheet_size_mm`
- `ups(machine.sheet_size_mm, format.trim_mm) >= 1`

No capable machine → `E201` (unpriceable; lint should have caught it).

Per capable machine:

```text
sheets = ceil_div(raw_sheets * (100 + waste_percent), 100) + waste_fixed_sheets
paper  = sheets * material.pricing.price_micro

digital:
    side_price(inks) = 0 if inks == 0; click_mono_micro if inks == 1; click_color_micro if inks >= 2
    cost = setup_micro + paper
         + sheets * (side_price(colors.front) + side_price(colors.back))

offset:
    plates = colors.front + colors.back          // one plate per ink per printed side
    cost = setup_micro + plates * plate_price_micro
         + sheets * (material.pricing.price_micro + run_price_micro)
```

Component result: `{ role, machine_id (None if unprinted), sheets, cost_micro }` for the cheapest machine. Tie → the machine that sorts first by record id (determinism).

### §6.3 Operation costing

Let `total_sheets = Σ sheets` over all component results.
Effective unit price `u = operation.unit_price_micro + material.pricing.price_micro` (material term is 0 if no `material` param; pricing basis ≠ `unit_basis` → `E204`). Let `edge = params.edge_mm ?? format.trim_mm[1]`, `area_mm2 = trim_mm[0] * trim_mm[1]`, `m = params.units_multiplier ?? 1` (must be an integer ≥ 1, else `E205`).

| `unit_basis` | cost formula (all integer) |
|---|---|
| `per_item`  | `setup + m * qty * u` |
| `per_sheet` | `setup + m * total_sheets * u` |
| `per_cm`    | `setup + round_half_up(m * qty * edge * u, 10)`  — edge is in mm, price per cm |
| `per_m2`    | `setup + round_half_up(m * qty * area_mm2 * u, 1_000_000)` — price per m² |

`units_multiplier` scales the material term too (double-sided lamination consumes double film) — that's why it multiplies `u`, not just labour. For non-linear cases (a duplex laminator whose second side isn't 2× labour), define a separate operation row instead.

### §6.4 Total, margin, rounding

```text
cost_micro  = Σ component costs + Σ operation costs
band        = margin band with largest min_qty <= qty
price_micro = round_half_up(cost_micro * band.multiplier_bp, 10_000)
total_minor = ceil_div(price_micro, 10_000)                         // µ → cents, round up
total_minor = ceil_div(total_minor, rounding.step_minor) * rounding.step_minor
total_minor = max(total_minor, min_price_minor)
unit_minor  = round_half_up(total_minor, qty)
```

Ladder = this computation per quantity, template ladder first, then validated custom quantities appended (deduplicated, ascending).

## §7 API contract

`GET /portal/products/:slug/schema` → `200` with parameters/options (+ `disabled` flags for the given selection, passed as query params, each with `reason: "unavailable" | "incompatible"` and the localized `unavailable_message` when present), defaults (select `is_default` options and numeric `default` values), quantity ladder, `custom_quantity`, `pricelist_version`.

`POST /portal/products/:slug/quote`
Request: `{ "selection": { ... }, "quantities": [750] }` (`quantities` optional).
Success `200`:

```json
{ "currency": "EUR", "pricelist_version": 42,
  "ladder": [ { "qty": 100, "total_minor": 29030, "unit_minor": 290 }, ... ] }
```

The authenticated back-office variant additionally returns `breakdown` per qty (component and operation rows with `cost_micro`, chosen machine, sheet counts). **Never** in the public portal response. Staff quoting (`docs/staff-quoting.md`) adds a direct-JobSpec entry point and three small engine deltas (normative JobSpec wire format, per-component machine pin, margin-override input, breakdown schema) — to be folded into §4/§6/§7 when implemented.

Errors (JSON body `{ "error": <CODE>, ... }`):

| HTTP | Code | When | Extra fields |
|---|---|---|---|
| 404 | `PRODUCT_NOT_FOUND` | unknown slug | |
| 400 | `INVALID_SELECTION` | §4.1 validation fails | `parameter`, `reason` |
| 400 | `INVALID_QUANTITY` | custom qty outside `custom_quantity` bounds | `qty`, `min`, `max` |
| 422 | `RULE_VIOLATION` | ≥1 rule violated | `violations: [{rule_id, message}]` (localized) |
| 500 | `TEMPLATE_ERROR` | resolution error E1xx on a saved template | log + alert; lint should prevent |
| 500 | `UNPRICEABLE` | pricing error E2xx | log + alert; lint should prevent |

Error code registry: `E101` unknown target role · `E102` duplicate `add_operation` · `E103` `set_op_param` on absent operation · `E104` `add_component` on existing role · `E105` empty technology allow-list · `E106` incomplete spec · `E107` dangling record reference · `E108` numeric `default` violates its `input` constraints · `E110` `$input` outside numeric parameter · `E201` no capable machine · `E202` item larger than sheet · `E204` material basis mismatch · `E205` invalid reserved-param value (`units_multiplier` < 1 or non-integer, `edge_mm` = 0).

## §8 Template lint (admin save-time)

Run on every template save; returns `errors` (block save) and `warnings`:

1. **Error**: default selection (all `is_default` options + numeric `default` values, falling back to `min` where absent) fails `resolve` or pricing.
2. **Error**: an `is_default` option has `available: false` (defaulted selections would start failing at quote time); a numeric `default` violates its `input` constraints (`E108`).
3. **Error**: for each parameter, each option substituted into the default selection fails `resolve` or pricing (linear in options).
4. **Warning**: two different parameters write the same `(target, attribute)`.
5. **Warning**: a randomized sample (fixed seed, 100 combos) of full selections has any resolution/pricing failure or rule contradiction (a rule violated by every sampled combo).
6. **Error**: any effect references a dangling record id (`E107` check across all options).

## §9 Golden fixture

The complete demo dataset, one selection, and exact expected numbers. Ship as `fixtures/demo.json` + `fixtures/expected.json`; the golden test loads both and compares to the engine output. **These numbers are normative** — a mismatch is an engine bug (or a deliberate, documented spec change).

### §9.1 Dataset

```jsonc
// formats
{ "id": "format:a6", "name": "A6", "trim_mm": [105, 148] }
{ "id": "format:a5", "name": "A5", "trim_mm": [148, 210] }
{ "id": "format:a4", "name": "A4", "trim_mm": [210, 297] }
{ "id": "format:dl", "name": "DL", "trim_mm": [99, 210] }

// materials — printable substrates (per_sheet, all SRA3 = [320, 450])
{ "id": "material:offset_80", "name": "Offset 80 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 40000 },
  "printable": { "grammage_gsm": 80 }, "attrs": {} }
{ "id": "material:offset_90", "name": "Offset 90 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 44000 },
  "printable": { "grammage_gsm": 90 }, "attrs": {} }
{ "id": "material:gloss_300", "name": "Gloss 300 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 120000 },
  "printable": { "grammage_gsm": 300 }, "attrs": {} }
{ "id": "material:matt_350", "name": "Matt 350 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 140000 },
  "printable": { "grammage_gsm": 350 }, "attrs": {} }
{ "id": "material:board_500", "name": "Board 500 g", "kind": "board",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 200000 },
  "printable": { "grammage_gsm": 500 }, "attrs": {} }
// board_500 exceeds every machine's grammage cap — printable in principle, but any
// attempt to print on it yields E201; the fixture uses it unprinted (0/0) only.

// materials — finishing consumables (no printable block)
{ "id": "material:spiral_black",  "name": "Spiral black",  "kind": "wire",
  "pricing": { "basis": "per_cm", "price_micro": 5000 }, "attrs": { "colour": "black" } }
{ "id": "material:spiral_silver", "name": "Spiral silver", "kind": "wire",
  "pricing": { "basis": "per_cm", "price_micro": 7000 }, "attrs": { "colour": "silver" } }
{ "id": "material:film_gloss",    "name": "Film gloss",    "kind": "film",
  "pricing": { "basis": "per_m2", "price_micro": 300000 }, "attrs": {} }
{ "id": "material:film_matt",     "name": "Film matt",     "kind": "film",
  "pricing": { "basis": "per_m2", "price_micro": 320000 }, "attrs": {} }

// machines
{ "id": "machine:digi1", "name": "Digital SRA3", "technology": "digital",
  "sheet_size_mm": [320,450], "duplex": true, "max_grammage_gsm": 350,
  "setup_micro": 2000000, "click_mono_micro": 8000, "click_color_micro": 60000,
  "waste_fixed_sheets": 10, "waste_percent": 2 }
{ "id": "machine:offset1", "name": "Offset SRA3", "technology": "offset",
  "sheet_size_mm": [320,450], "duplex": true, "max_grammage_gsm": 400,
  "setup_micro": 15000000, "plate_price_micro": 8000000, "run_price_micro": 15000,
  "waste_fixed_sheets": 150, "waste_percent": 2 }

// operations
{ "id": "operation:cutting",        "setup_micro": 3000000, "unit_basis": "per_item", "unit_price_micro": 10000 }
{ "id": "operation:spiral_binding", "setup_micro": 5000000, "unit_basis": "per_cm",   "unit_price_micro": 4000 }
{ "id": "operation:prepress",       "setup_micro": 4000000, "unit_basis": "per_item", "unit_price_micro": 0 }
{ "id": "operation:lamination",     "setup_micro": 6000000, "unit_basis": "per_m2",   "unit_price_micro": 200000 }

// pricing policy: as the example in §2 (bands ×1.7 / ×1.6 @250 / ×1.5 @1000,
// rounding step 10 up, min_price_minor 2500, currency EUR)

// template: spiral-notebook exactly as in product-configuration.md (canonical field
// names), i.e. base_effects = [ set_pages cover 2, add spiral_binding, add cutting,
// add prepress ]; rule: spiral_binding requires component:interior.pages >= 20
```

### §9.2 Request

```json
POST /portal/products/spiral-notebook/quote
{ "selection": { "format": "a5", "printing": "4_0", "sheets": "50",
                 "cover_stock": "gloss300", "interior_stock": "offset80",
                 "spiral_color": "black", "lamination": "none", "backing": "yes" } }
```

### §9.3 Expected resolved JobSpec

```json
{ "format": "format:a5", "quantity": "<per ladder entry>",
  "components": [
    { "role": "cover",    "pages": 2,   "colors": "4/0", "material": "material:gloss_300" },
    { "role": "interior", "pages": 100, "colors": "4/0", "material": "material:offset_80" },
    { "role": "backing",  "pages": 2,   "colors": "0/0", "material": "material:board_500" } ],
  "operations": [
    { "operation": "operation:spiral_binding", "params": { "material": "material:spiral_black" } },
    { "operation": "operation:cutting",  "params": {} },
    { "operation": "operation:prepress", "params": {} } ],
  "technology_allow": null }
```

### §9.4 Expected computation, qty = 100

`ups(A5) = 4` (§6.1). All amounts in µ.

| Step | Derivation | Value |
|---|---|---:|
| interior leaves / raw | `ceil(100/2)=50`; `ceil(100·50/4)` | 1 250 sheets |
| interior on digi1 | sheets `ceil(1250·102/100)+10 = 1285`; paper `1285·40000`; clicks front colour `1285·60000`; setup | 2 000 000 + 51 400 000 + 77 100 000 = **130 500 000** |
| interior on offset1 | sheets `1275+150 = 1425`; plates 4; `15M + 4·8M + 1425·(40000+15000)` | 15 000 000 + 32 000 000 + 78 375 000 = **125 375 000** ← wins |
| cover leaves / raw | `ceil(2/2)=1`; `ceil(100·1/4)` | 25 sheets |
| cover on digi1 | sheets `ceil(25·102/100)+10 = 36`; `2M + 36·120000 + 36·60000` | **8 480 000** ← wins |
| cover on offset1 | sheets `26+150 = 176`; `15M + 32M + 176·135000` | 70 760 000 |
| backing (0/0) | `ups=4`; `ceil(100·1/4)=25`; `25·200000`; no waste, no machine | **5 000 000** |
| spiral_binding | u = 4000+5000; `5M + round(100·210·9000, 10)` | 5 000 000 + 18 900 000 = **23 900 000** |
| cutting | `3M + 100·10000` | **4 000 000** |
| prepress | `4M + 100·0` | **4 000 000** |
| **cost_micro** | 125 375 000 + 8 480 000 + 5 000 000 + 23 900 000 + 4 000 000 + 4 000 000 | **170 755 000** |
| margin | band `min_qty 1` → ×1.7: `round(170755000·17000, 10000)` | 290 283 500 |
| total_minor | `ceil(290283500/10000)=29029`; step 10 up | **29 030** (€290.30) |
| unit_minor | `round_half_up(29030, 100)` | **290** (€2.90) |

### §9.5 Expected computation, qty = 1000

| Step | Value (µ) |
|---|---:|
| interior: raw 12 500; digital `2M + 12760·40000 + 12760·60000 = 1 278 000 000`; offset sheets `12750+150=12900`, `47M + 12900·55000` = **756 500 000** ← offset wins | 756 500 000 |
| cover: raw 250; digital sheets `255+10=265`, `2M + 265·180000` = **49 700 000** ← digital wins; offset `47M + 405·135000 = 101 675 000` | 49 700 000 |
| backing: `ceil(1000/4)=250`, `250·200000` | 50 000 000 |
| spiral: `5M + round(1000·210·9000, 10)` | 194 000 000 |
| cutting: `3M + 1000·10000` | 13 000 000 |
| prepress | 4 000 000 |
| **cost_micro** | **1 067 200 000** |
| margin band `min_qty 1000` → ×1.5 | 1 600 800 000 |
| total_minor (exact, already on step) | **160 080** (€1 600.80) |
| unit_minor | **160** (€1.60) |

Note the fixture deliberately exercises the technology breakpoint: the interior runs offset at both quantities (1 250 sheets is already a long run), the cover runs digital at both, and unit price falls €2.90 → €1.60.

### §9.6 Expected response (assertable entries)

```json
{ "currency": "EUR", "pricelist_version": 1,
  "ladder": [ { "qty": 100,  "total_minor": 29030,  "unit_minor": 290 },
              { "qty": 1000, "total_minor": 160080, "unit_minor": 160 } ] }
```

(The full response contains all 7 ladder quantities; the golden test MUST assert at minimum these two entries exactly, and SHOULD snapshot the full ladder on first run.)

## §10 Required tests

1. **Golden test** — §9 end-to-end: dataset → resolve → price → assert §9.4/§9.5 numbers including intermediate `cost_micro` per component/operation (expose a breakdown for this).
2. **Ups table** — §6.1 reference values A6/DL/A5/A4.
3. **Resolution errors** — one test per code E101–E110 with a minimal broken template each.
4. **Rule evaluation** — violated / satisfied / missing-component-evaluates-false; `when`-absent behaves as always-on.
5. **Selection validation** — each §4.1 case; defaults applied for missing select *and* numeric parameters; `available: false` rejected with `option_unavailable` both when chosen explicitly and when reached via default (the latter is a lint error, but the API must still reject).
6. **Fixture ladder monotonicity** — on the §9 dataset, for qty 50..2000 step 50: `total_minor` non-decreasing, and `unit_minor` non-increasing across the template's ladder quantities. (Unit price is *not* universally monotone at small quantities because of sheet-count ceilings — assert it only on the fixture ladder, not as a general property.)
7. **Determinism** — pricing the same request twice yields identical bytes; machine tie-break by record id.
8. **No-float check** — CI greps `crates/quote-engine/src` for `f32`/`f64` and fails on match.
9. **Units multiplier** — for each of the four bases: `units_multiplier: 2` exactly doubles the unit term (including the material contribution) and leaves `setup_micro` unchanged; `0` and non-integer values → `E205`; absent behaves as `1`.
