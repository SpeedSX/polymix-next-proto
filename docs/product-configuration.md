# Product Configuration Model — Preliminary Design

How a tenant defines configurator parameters and options as data, and how a customer-facing choice like "double-sided 4/4 (cover and interior)" or "4+8 pages with cover" turns into something the pricing engine (`docs/instant-quote.md`) can cost.

> **Normative reference:** exact effect serialization, resolution semantics, costing formulas, and a golden test fixture live in `docs/quote-engine-spec.md`. Where that spec and this narrative disagree, the spec wins.

## The core problem

Options and costs live in different vocabularies:

- The **customer** sees marketing-shaped choices: `PRINTING: one-sided 4/0 (cover and interior)`, `NUMBER OF PAGES WITH COVER: 4+8`.
- The **engine** needs physical facts: cover has 4 pages in 4/4, interior has 8 pages in 4/4 on 60 g newsprint, then it can impose, pick a technology, and cost the job.

One option frequently sets several facts at once (a *composite option*), and the same fact can be set by different parameters in different products. So we never map options to prices, and never hardcode option semantics — we map options to **effects**, and effects write into a **job spec**.

```
tenant-defined                 declarative bridge        engine-owned, fixed
┌──────────────────┐          ┌────────────────┐        ┌──────────────────┐
│ product_template │          │ effects        │        │ JobSpec          │
│  parameters      │──select──▶  set_format    │──write─▶  format, qty     │
│   options        │          │  set_colors    │        │  components[]    │
│    labels (i18n) │          │  set_pages ... │        │  operations[]    │
└──────────────────┘          └────────────────┘        └──────────────────┘
```

The contract that keeps this maintainable: **the effect vocabulary and job-spec attributes are defined by the engine (code, typed, versioned); everything above them — products, parameters, options, labels, which effects an option carries — is tenant data.** A tenant can add a new paper, a new option, a whole new product without a deploy; a genuinely new *kind* of physical consequence (a new effect kind) is engine work, deliberately.

## Layer 1 — JobSpec (engine input, fixed vocabulary)

```rust
struct JobSpec {
    format: TrimSize,                  // A4, A5, 285×400 …
    quantity: u32,
    components: Vec<Component>,        // roles defined by the template
    operations: Vec<OperationInstance>,
    technology_allow: Option<Vec<TechnologyId>>,
}

struct Component {
    role: String,                      // "cover", "interior"
    pages: u32,                        // logical pages (1 leaf = 2 pages)
    colors: Colors,                    // front/back ink counts: 4/0, 4/4, 1/1
    material: MaterialId,              // per_sheet-priced material (see quote-engine-spec §2)
}

struct OperationInstance {
    operation: OperationId,            // spiral_binding, lamination, cutting …
    params: Map<String, Value>,        // e.g. spiral_material, lamination_film
}
```

A spec is **complete** when every component has format-derivable imposition, pages, colors, and material, and quantity is set. The engine refuses incomplete specs — completeness is checked at template-lint time (below), so customers never hit it.

## Layer 2 — Effects (the bridge, a closed tagged enum)

| Effect | Payload | Example use |
|---|---|---|
| `set_format` | format ref (resolves to trim size) | `FORMAT: A5`, `FORMAT: DL` |
| `set_pages` | target component, pages | `4+8` → interior.pages = 8 |
| `set_colors` | target component, front/back | `4/4` → cover.colors = 4/4 |
| `set_material` | target component, material ref | `COVER: 300 g gloss` |
| `add_operation` | operation ref, params | `LAMINATION: matt` |
| `set_op_param` | operation ref, param, value | `SPIRAL COLOR: silver` → material of the already-added binding op |
| `add_component` | role, pages, colors, material | `BACKING: cardboard` adds a third component |
| `constrain_technology` | allow-list | force digital for variable data |

Serialized as a serde-tagged enum, so Rust gets exhaustive matching and the admin UI gets a finite, form-buildable list. Application order: template `base_effects` first, then each selected option's effects in parameter display order; later writes to the same attribute win (and template lint warns when two parameters write the same attribute unintentionally).

## Layer 3 — Product template (tenant data, SurrealDB)

```
product_template: slug, name{i18n}, component roles, base_effects, qty ladder
  └─ parameter:   code, label{i18n}, position, kind: select|numeric
       └─ option: code, label{i18n}, position, is_default, available, effects[]
compatibility_rule: template ref, expression, message{i18n}
```

`base_effects` carry what *every* variant of the product shares — a spiral notebook always has spiral binding and cutting; those are not options.

## Worked example 1 — spiral notebook

```jsonc
{
  "slug": "spiral-notebook",
  "components": ["cover", "interior"],
  "base_effects": [
    { "kind": "set_pages", "target": "cover", "value": 2 },
    { "kind": "add_operation", "operation": "operation:spiral_binding" },
    { "kind": "add_operation", "operation": "operation:cutting" },
    { "kind": "add_operation", "operation": "operation:prepress" }
  ],
  "quantities": [50, 100, 200, 300, 500, 1000, 2000],
  "parameters": [
    { "code": "format", "label": { "en": "Format", "pl": "Format" }, "options": [
      { "code": "a6", "effects": [{ "kind": "set_format", "format": "format:a6" }] },
      { "code": "a5", "is_default": true,
        "effects": [{ "kind": "set_format", "format": "format:a5" }] },
      { "code": "a4", "effects": [{ "kind": "set_format", "format": "format:a4" }] },
      { "code": "dl", "effects": [{ "kind": "set_format", "format": "format:dl" }] }
    ]},

    // the composite option from the question: one choice, two components written
    { "code": "printing", "label": { "en": "Printing", "pl": "Zadruk" }, "options": [
      { "code": "4_0",
        "label": { "en": "one-sided 4/0 (cover and interior)" },
        "effects": [
          { "kind": "set_colors", "target": "cover",    "value": "4/0" },
          { "kind": "set_colors", "target": "interior", "value": "4/0" }
        ]},
      { "code": "4_4", "is_default": true,
        "label": { "en": "double-sided 4/4 (cover and interior)" },
        "effects": [
          { "kind": "set_colors", "target": "cover",    "value": "4/4" },
          { "kind": "set_colors", "target": "interior", "value": "4/4" }
        ]}
    ]},

    { "code": "sheets", "label": { "en": "Number of sheets" }, "options": [
      { "code": "50",  "effects": [{ "kind": "set_pages", "target": "interior", "value": 100 }] },
      { "code": "80",  "effects": [{ "kind": "set_pages", "target": "interior", "value": 160 }] },
      { "code": "100", "effects": [{ "kind": "set_pages", "target": "interior", "value": 200 }] }
    ]},

    { "code": "cover_stock", "label": { "en": "Cover" }, "options": [
      { "code": "gloss300", "effects": [{ "kind": "set_material", "target": "cover", "material": "material:gloss_300" }] },
      { "code": "matt350",  "effects": [{ "kind": "set_material", "target": "cover", "material": "material:matt_350" }] }
    ]},

    { "code": "interior_stock", "label": { "en": "Interior paper" }, "options": [
      { "code": "offset80", "effects": [{ "kind": "set_material", "target": "interior", "material": "material:offset_80" }] },
      { "code": "offset90", "effects": [{ "kind": "set_material", "target": "interior", "material": "material:offset_90" }] }
    ]},

    // options that only tweak an operation already added by base_effects
    { "code": "spiral_color", "label": { "en": "Spiral colour" }, "options": [
      { "code": "black",  "effects": [{ "kind": "set_op_param", "operation": "operation:spiral_binding",
                                        "param": "material", "value": "material:spiral_black" }] },
      { "code": "silver", "effects": [{ "kind": "set_op_param", "operation": "operation:spiral_binding",
                                        "param": "material", "value": "material:spiral_silver" }] }
    ]},

    { "code": "lamination", "label": { "en": "Cover lamination" }, "options": [
      { "code": "none",  "effects": [] },
      { "code": "gloss", "effects": [{ "kind": "add_operation", "operation": "operation:lamination",
                                       "params": { "material": "material:film_gloss", "target": "cover" } }] },
      { "code": "matt",  "effects": [{ "kind": "add_operation", "operation": "operation:lamination",
                                       "params": { "material": "material:film_matt", "target": "cover" } }] },
      // reserved param units_multiplier scales the unit term (film + labour), not the setup —
      // one changeover regardless of sides (quote-engine-spec §6.3)
      { "code": "matt_2s", "label": { "en": "double-sided, matt" },
        "effects": [{ "kind": "add_operation", "operation": "operation:lamination",
                      "params": { "material": "material:film_matt", "target": "cover",
                                  "units_multiplier": 2 } }] }
    ]},

    // an option that introduces a whole new component
    { "code": "backing", "label": { "en": "Cardboard backing" }, "options": [
      { "code": "yes", "is_default": true,
        "effects": [{ "kind": "add_component", "role": "backing",
                      "pages": 2, "colors": "0/0", "material": "material:board_500" }] },
      { "code": "no", "effects": [] }
    ]}
  ]
}
```

Note what the composite option does: the label *"one-sided 4/0 (cover and interior)"* is pure presentation; the semantics live entirely in its two `set_colors` effects. If the tenant later wants a cheaper variant *"cover 4/0, interior 1/1"*, they add an option with two different effects — no code change, and the engine prices it correctly because interior mono clicks are cheaper than color clicks.

### Formats are data too — what DL demonstrates

`set_format` references a tenant-maintained format registry, not a hardcoded enum, so a non-ISO trim is just another row:

```jsonc
{ "id": "format:a6", "name": "A6", "trim_mm": [105, 148] }
{ "id": "format:a5", "name": "A5", "trim_mm": [148, 210] }
{ "id": "format:a4", "name": "A4", "trim_mm": [210, 297] }
{ "id": "format:dl", "name": "DL", "trim_mm": [99, 210] }   // non-ISO, no code change
```

The option only sets the trim; **imposition is engine math**, computed from trim + bleed against the press-sheet size, trying both orientations. On SRA3 (320 × 450 mm) with 3 mm bleed:

| Format | Footprint (trim + 2×bleed) | Best layout | Ups |
|---|---|---|---:|
| A6 | 111 × 154 | 4 × 2 | 8 |
| DL | 105 × 216 | 3 × 2 (rotated) | 6 |
| A5 | 154 × 216 | 2 × 2 | 4 |
| A4 | 216 × 303 | 2 × 1 | 2 |

So DL prices naturally between A6 and A5 without anyone entering a DL price anywhere — the 6-up layout only exists because the engine tried the rotated orientation. This is the payoff of keeping format physical: a tenant adding a custom 120 × 120 sticker-book format gets correct prices immediately.

One subtlety DL surfaces: operation costs may depend on resolved geometry. Spiral consumption scales with the bound edge, so `operation:spiral_binding` defines its unit cost **per cm of bound edge** (`unit_basis: per_cm`), and the engine feeds it the edge length from the resolved format (210 mm for DL/A5 long-edge... or 99 mm if the template binds DL on the short edge — a `set_op_param(binding_edge)` effect on the format option can express that). Same mechanism prices lamination per m² rather than per item.

## Worked example 2 — newspaper, `NUMBER OF PAGES WITH COVER`

The `4+4 / 4+8 / 4+12` pattern is the same composite-option mechanism writing `pages` on two components:

```jsonc
{
  "slug": "newspaper",
  "components": ["cover", "interior"],
  "base_effects": [
    { "kind": "add_operation", "operation": "operation:saddle_fold" },
    { "kind": "set_colors", "target": "cover", "value": "4/4" },
    { "kind": "set_material",  "target": "cover", "material": "material:newsprint_60" }
  ],
  "parameters": [
    { "code": "pages_with_cover", "label": { "en": "Number of pages with cover" }, "options": [
      { "code": "4_4",  "label": { "en": "4+4" },
        "effects": [{ "kind": "set_pages", "target": "cover", "value": 4 },
                    { "kind": "set_pages", "target": "interior", "value": 4 }] },
      { "code": "4_8",  "label": { "en": "4+8" },
        "effects": [{ "kind": "set_pages", "target": "cover", "value": 4 },
                    { "kind": "set_pages", "target": "interior", "value": 8 }] },
      { "code": "4_12", "label": { "en": "4+12" },
        "effects": [{ "kind": "set_pages", "target": "cover", "value": 4 },
                    { "kind": "set_pages", "target": "interior", "value": 12 }] }
    ]},
    { "code": "interior_printing", "label": { "en": "Interior printing" }, "options": [
      { "code": "1_1", "effects": [{ "kind": "set_colors", "target": "interior", "value": "1/1" }] },
      { "code": "4_4", "effects": [{ "kind": "set_colors", "target": "interior", "value": "4/4" }] }
    ]}
    // format, interior material … same patterns as above
  ]
}
```

Two products, zero product-specific engine code: the notebook and the newspaper differ only in data.

## Resolution pipeline (per quote request)

1. Start from an empty spec with the template's component roles; apply `base_effects`.
2. Apply the effects of each selected option, in parameter order.
3. Evaluate `compatibility_rule`s against the resolved spec (`spiral_binding requires interior.pages >= 20`; `lamination excludes cover.material in [uncoated…]`). Rules reference *spec attributes*, not option codes, so they keep working when options are added.
4. Completeness check → hand `JobSpec` to the pricing engine for each ladder quantity.

The same rule evaluation backs the `GET /schema` endpoint: for the current selection, re-resolve with each candidate option substituted and mark options whose spec would violate a rule — that's how the UI greys out invalid combinations without a custom matrix.

## Admin experience (how a tenant maintains this)

- **Template editor** (Mantine screens): parameter list with drag-ordering; per option a small **effect builder** — effect kind from a fixed dropdown, target from the template's component roles, value pickers constrained by type (material picker shows only printable, per-sheet-priced materials within the target's format/grammage limits).
- **Lint on save**, not at quote time: resolve defaults, then defaults-with-each-single-option-substituted (linear in options, not the cartesian product), plus a randomized sample of full combinations. Report incomplete specs, unintended attribute overwrites, rule contradictions, and unpriceable specs (e.g. no machine can run 500 g board). Property test in CI does the same over demo tenants.
- **Availability toggle**: each option carries `available: bool` — a standing "temporarily can't fulfill this" flag (out of material, machine down) with an optional tooltip message. The portal greys the option out with the message; the quote API rejects it regardless. Distinct from compatibility rules, which depend on the current selection.
- **Cloning**: new products start as a copy of an existing template — in practice tenants tweak, they don't author from scratch.

## What a tenant admin can build alone — and where the boundary is

The dividing line: **new combinations of known physics are tenant data; new kinds of physics are engine releases.** Concretely, per product family:

### Custom books — fully self-serve

Books are what the vocabulary was built for: components (`cover`, `interior`, plus `endpapers` / `dust_jacket` via `add_component`), pages/colors/material per component, a binding operation. A lay-flat photo book is: create `operation:layflat_binding` (setup + unit cost, e.g. per cm of spine — all data), clone the nearest template, swap the binding in `base_effects`, adjust parameters, fix what lint flags. No deploy.

### New finishing processes — self-serve in almost all cases

A finishing process is an `operation` row: setup + unit cost + unit basis. Foil stamping, embossing, drilling, perforation, numbering, corner rounding all fit the `setup + qty × unit` shape; process-specific variability (number of drill holes, foil colour) is op params set by options via `set_op_param`. Engine work is needed only when the cost driver isn't among the unit bases (per item / sheet / cut / cm / m²) — and adding a basis is a small, reviewed vocabulary change, the intended growth path.

### Packaging — the real boundary, in two tiers

- **Fixed-size packaging** (standard mailer box, folder with flaps) — workable today with a mild workaround: the admin computes the die-cut blank size, enters it as a custom format (the blank is a rectangular footprint for imposition), and models die-cutting as an operation whose setup cost carries the die. Rectangular footprint over-estimates sheet usage versus true nested die layouts — conservative, prices err safe, but not optimal.
- **Customer-parametric packaging** (customer enters L×W×H, blank computed with flap allowances) — **not expressible** in the current vocabulary: `set_format` picks a stored trim, and no effect *derives* geometry from numeric inputs. Requires a new engine capability (derived-geometry functions or the recorded expression-language escape hatch); true die-layout nesting is a further step beyond footprint imposition.

The same boundary appears for roll/wide-format products (banners priced per m² of print, not per sheet): a per-area technology model next to the sheet/click one — engine work, a vocabulary extension rather than a redesign.

### Summary table

| Ask | Self-serve? | What it takes |
|---|---|---|
| New product from existing components/operations | ✅ | Clone template, edit parameters/options |
| New material (any family: paper, film, wire, foil…), format (incl. non-ISO), margin policy | ✅ | Add a row |
| New finishing process, `setup + qty × unit` shaped | ✅ | Add an `operation` row, reference it from options |
| Fixed-size packaging | ⚠️ | Pre-compute blank as custom format; conservative pricing |
| Parametric geometry (L×W×H boxes), die-layout nesting | ❌ | Engine release: derived-geometry effects |
| Per-area (roll/wide-format) printing | ❌ | Engine release: per-m² technology model |
| New cost basis for an operation | ❌ (small) | Engine release: add a unit basis |

## Deliberate limits and later extensions

- **Closed effect vocabulary** — no per-tenant formulas or scripting in v1. If a tenant need genuinely doesn't fit, that's a signal to grow the vocabulary (reviewed engine change), not to open an escape hatch. A sandboxed expression language (e.g. Rhai) stays a recorded option if the vocabulary sprawls.
- **Numeric parameters** (custom page count, custom format) use the same effects with the entered value substituted; ranges/steps and a `default` (used when the customer hasn't touched the control) are validated on the parameter definition.
- **Matrix price overrides** (see instant-quote.md) key on resolved spec hashes, so they survive option renames.
