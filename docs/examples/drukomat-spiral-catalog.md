# Worked example — "Katalogi spiralowane" (Drukomat.pl)

A real competitor product, mapped onto the schema in `docs/quote-engine-spec.md`, to sanity-check the spec against a live configurator rather than an invented one. Source: a saved rendering of `https://www.drukomat.pl/katalog-spiralowany` (Vue component state inline in the HTML — `data-property-id`/`data-property-code` attributes and radio option lists), captured 2026-07-05.

**What's real vs. illustrative:**
- Formats, paper combinations, page range, lamination options, spiral colours, file-check options, and the qty ladder + net price table below are read verbatim from the saved page.
- `price_micro` values on new material/operation rows, and the `pricing_policy` band, are **placeholders** — Drukomat's internal cost basis isn't public. Don't treat those numbers as real.

This template uses the normative field names from `quote-engine-spec.md` (`is_default`, `available`, `unavailable_message`, `units_multiplier`), not the narrative doc's shorthand.

## Source data (as captured)

| Property (site id) | code | Options (site value id) |
|---|---|---|
| format (2) | `format` | A6 pion 105×148 (568) · A5 poziom 210×148 (560) · **A5 pion 148×210 (259, current)** · A4 poziom 297×210 (519) · A4 pion 210×297 (285) · DL pion 99×210 (571) |
| rodzaj papieru (3) | `rodzaj-papieru` | **mat 250g/mat 130g (574, current)** · połysk 250g/połysk 130g (576) · połysk 250g/połysk 170g (535) · mat 350g/mat 170g (523) |
| ilość stron z okładką (4) | `pages` | 4+8 (314) … **4+20 (586, current)** … 4+56 (533), step 4 |
| lakierowanie/foliowanie okładki (5) | `lakier_folia` | **standard/none (276, current)** · folia mat (530) · folia połysk (537, **disabled on site**) |
| kolor spirali (6) | `spiral_color` | biała (629) · **czarna (628, current)** · srebrna (531) |
| Sprawdzanie plików (manualcheck) | `manual_check` | **sprawdzanie automatyczne (0, current)** · sprawdzanie przez konsultanta (1) |
| zadruk (7, constant) | — | dwustronny 4/4 — only ever this value, so folded into `base_effects` below rather than exposed as a parameter |

Quantities on the site: 50 (default), 100…900 (step 100), 1000, 1500…5000 (step 500); custom quantity up to 5000.

Observed net price ladder for the default combination (A5 portrait / mat 250+130 / 4+20 pages / no lamination / black spiral / auto file check):

| qty | net (zł) | qty | net (zł) | qty | net (zł) |
|---:|---:|---:|---:|---:|---:|
| 50 | 1,643.25 | 500 | 2,439.15 | 2000 | 5,172.30 |
| 100 | 1,731.45 | 600 | 2,642.85 | 2500 | 6,091.05 |
| 200 | 1,905.75 | 700 | 2,817.15 | 3000 | 6,998.25 |
| 300 | 2,086.35 | 800 | 2,997.75 | 3500 | 7,886.55 |
| 400 | 2,259.60 | 900 | 3,171.00 | 4000 | 8,799.00 |
| | | 1000 | 3,348.45 | 4500 | 9,709.35 |
| | | 1500 | 4,264.05 | 5000 | 10,627.05 |

A linear fit `total ≈ 1549.9 + 1.815·qty` matches every point to within ~0.8%, with no visible step at qty 250 or 1000 — this is why `pricing_policy:spiral_catalog` below uses a single margin band rather than copying the 3-band schedule from the `quote-engine-spec.md` §9 golden fixture (that fixture's bands are a test dataset, not a suggested default).

## New catalog rows

```jsonc
// materials — cover stock (per_sheet, SRA3 = [320,450]); price_micro is a placeholder
{ "id": "material:cover_mat_250",   "name": "Kreda mat 250 g",    "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 100000 },
  "printable": { "grammage_gsm": 250 }, "attrs": { "finish": "matte" } }
{ "id": "material:cover_gloss_250", "name": "Kreda połysk 250 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 100000 },
  "printable": { "grammage_gsm": 250 }, "attrs": { "finish": "gloss" } }
{ "id": "material:cover_mat_350",   "name": "Kreda mat 350 g",    "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 135000 },
  "printable": { "grammage_gsm": 350 }, "attrs": { "finish": "matte" } }

// materials — interior stock (per_sheet, SRA3)
{ "id": "material:interior_mat_130",   "name": "Kreda mat 130 g",    "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 55000 },
  "printable": { "grammage_gsm": 130 }, "attrs": { "finish": "matte" } }
{ "id": "material:interior_gloss_130", "name": "Kreda połysk 130 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 55000 },
  "printable": { "grammage_gsm": 130 }, "attrs": { "finish": "gloss" } }
{ "id": "material:interior_gloss_170", "name": "Kreda połysk 170 g", "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 62000 },
  "printable": { "grammage_gsm": 170 }, "attrs": { "finish": "gloss" } }
{ "id": "material:interior_mat_170",   "name": "Kreda mat 170 g",    "kind": "paper",
  "pricing": { "basis": "per_sheet", "sheet_size_mm": [320,450], "price_micro": 62000 },
  "printable": { "grammage_gsm": 170 }, "attrs": { "finish": "matte" } }

// material — third spiral colour (spiral_black / spiral_silver already exist in the §9 fixture)
{ "id": "material:spiral_white", "name": "Spiral white", "kind": "wire",
  "pricing": { "basis": "per_cm", "price_micro": 5000 }, "attrs": { "colour": "white" } }

// formats — a6/a5/a4/dl already exist in the §9 fixture with these exact trim_mm; the two
// new rows are landscape *labels* only, same physical rectangle, same imposition/price
{ "id": "format:a5_landscape", "name": "A5 poziom", "trim_mm": [148, 210] }
{ "id": "format:a4_landscape", "name": "A4 poziom", "trim_mm": [210, 297] }

// operation — file-check-by-consultant surcharge; setup_micro is a placeholder
{ "id": "operation:manual_check", "setup_micro": 15000000, "unit_basis": "per_item", "unit_price_micro": 0 }

// pricing policy — single band, chosen to reproduce the observed smooth ladder (see fit above);
// multiplier_bp/rounding/min_price_minor are placeholders, currency PLN to match the source
{ "id": "pricing_policy:spiral_catalog", "currency": "PLN",
  "margin_bands": [ { "min_qty": 1, "multiplier_bp": 17000 } ],
  "rounding": { "step_minor": 10, "mode": "up" },
  "min_price_minor": 2500 }
```

## Template

```jsonc
{
  "id": "product_template:spiral_catalog",
  "slug": "katalog-spiralowany",
  "name": { "en": "Spiral catalog", "pl": "Katalog spiralowany" },
  "components": ["cover", "interior"],
  "pricing_policy": "pricing_policy:spiral_catalog",
  "quantities": [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1500, 2000, 2500, 3000, 3500, 4000, 4500, 5000],
  "custom_quantity": { "min": 50, "max": 5000 },

  // "zadruk: dwustronny 4/4" is the site's only constant parameter — never a customer
  // choice, so it belongs in base_effects rather than as a one-option parameter
  "base_effects": [
    { "kind": "set_pages",  "target": "cover", "value": 4 },
    { "kind": "set_colors", "target": "cover",    "value": "4/4" },
    { "kind": "set_colors", "target": "interior", "value": "4/4" },
    { "kind": "add_operation", "operation": "operation:spiral_binding" },
    { "kind": "add_operation", "operation": "operation:cutting" },
    { "kind": "add_operation", "operation": "operation:prepress" }
  ],

  "parameters": [
    { "code": "format", "label": { "en": "Format", "pl": "Format" }, "kind": "select", "options": [
      { "code": "a6",           "label": { "pl": "A6 pion (105 x 148 mm)" },
        "effects": [{ "kind": "set_format", "format": "format:a6" }] },
      { "code": "a5_landscape", "label": { "pl": "A5 poziom (210 x 148 mm)" },
        "effects": [{ "kind": "set_format", "format": "format:a5_landscape" }] },
      { "code": "a5",           "label": { "pl": "A5 pion (148 x 210 mm)" }, "is_default": true,
        "effects": [{ "kind": "set_format", "format": "format:a5" }] },
      { "code": "a4_landscape", "label": { "pl": "A4 poziom (297 x 210 mm)" },
        "effects": [{ "kind": "set_format", "format": "format:a4_landscape" }] },
      { "code": "a4",           "label": { "pl": "A4 pion (210 x 297 mm)" },
        "effects": [{ "kind": "set_format", "format": "format:a4" }] },
      { "code": "dl",           "label": { "pl": "DL pion (99 x 210 mm)" },
        "effects": [{ "kind": "set_format", "format": "format:dl" }] }
    ]},

    // curated combos, not a weight × finish cross-product — the site never offers
    // mismatched finishes (e.g. mat cover with gloss interior), so one flattened
    // select with composite effects is the only way to represent this faithfully
    { "code": "rodzaj-papieru", "label": { "en": "Paper", "pl": "Rodzaj papieru" }, "kind": "select", "options": [
      { "code": "mat250_mat130", "is_default": true,
        "label": { "pl": "kreda mat 250 g (okładka), kreda mat 130 g (wnętrze)" },
        "effects": [
          { "kind": "set_material", "target": "cover",    "material": "material:cover_mat_250" },
          { "kind": "set_material", "target": "interior", "material": "material:interior_mat_130" } ]},
      { "code": "gloss250_gloss130",
        "label": { "pl": "kreda połysk 250 g (okładka), kreda połysk 130 g (wnętrze)" },
        "effects": [
          { "kind": "set_material", "target": "cover",    "material": "material:cover_gloss_250" },
          { "kind": "set_material", "target": "interior", "material": "material:interior_gloss_130" } ]},
      { "code": "gloss250_gloss170",
        "label": { "pl": "kreda połysk 250 g (okładka), kreda połysk 170 g (wnętrze)" },
        "effects": [
          { "kind": "set_material", "target": "cover",    "material": "material:cover_gloss_250" },
          { "kind": "set_material", "target": "interior", "material": "material:interior_gloss_170" } ]},
      { "code": "mat350_mat170",
        "label": { "pl": "kreda mat 350 g (okładka), kreda mat 170 g (wnętrze)" },
        "effects": [
          { "kind": "set_material", "target": "cover",    "material": "material:cover_mat_350" },
          { "kind": "set_material", "target": "interior", "material": "material:interior_mat_170" } ]}
    ]},

    { "code": "pages", "label": { "en": "Interior pages", "pl": "Ilość stron z okładką" }, "kind": "numeric",
      "input": { "min": 8, "max": 56, "step": 4 }, "default": 20,
      "effects": [{ "kind": "set_pages", "target": "interior", "value": { "$input": {} } }] },

    { "code": "lakier_folia", "label": { "en": "Cover finish", "pl": "Lakierowanie, foliowanie okładki" }, "kind": "select", "options": [
      { "code": "standard", "label": { "pl": "standard" }, "is_default": true, "effects": [] },
      { "code": "folia_mat", "label": { "pl": "folia mat" },
        "effects": [{ "kind": "add_operation", "operation": "operation:lamination",
                      "params": { "material": "material:film_matt" } }] },
      { "code": "folia_polysk", "label": { "pl": "folia połysk" },
        "available": false,
        "unavailable_message": { "pl": "Chwilowo niedostępne", "en": "Temporarily unavailable" },
        "effects": [{ "kind": "add_operation", "operation": "operation:lamination",
                      "params": { "material": "material:film_gloss" } }] }
      // single-sided only on this product, so units_multiplier is left at its default (1);
      // a "matt, double-sided" variant would add "units_multiplier": 2 to its params (§6.3)
    ]},

    { "code": "spiral_color", "label": { "en": "Spiral colour", "pl": "Kolor spirali" }, "kind": "select", "options": [
      { "code": "biala",  "label": { "pl": "biała" },
        "effects": [{ "kind": "set_op_param", "operation": "operation:spiral_binding",
                      "param": "material", "value": "material:spiral_white" }] },
      { "code": "czarna", "label": { "pl": "czarna" }, "is_default": true,
        "effects": [{ "kind": "set_op_param", "operation": "operation:spiral_binding",
                      "param": "material", "value": "material:spiral_black" }] },
      { "code": "srebrna", "label": { "pl": "srebrna" },
        "effects": [{ "kind": "set_op_param", "operation": "operation:spiral_binding",
                      "param": "material", "value": "material:spiral_silver" }] }
    ]},

    { "code": "manual_check", "label": { "en": "File checking", "pl": "Sprawdzanie plików" }, "kind": "select", "options": [
      { "code": "auto",       "label": { "pl": "sprawdzanie automatyczne" }, "is_default": true, "effects": [] },
      { "code": "consultant", "label": { "pl": "sprawdzanie przez konsultanta" },
        "effects": [{ "kind": "add_operation", "operation": "operation:manual_check" }] }
    ]}
  ]
}
```

## Default selection → resolved JobSpec

```json
{ "selection": { "format": "a5", "rodzaj-papieru": "mat250_mat130", "pages": 20,
                 "lakier_folia": "standard", "spiral_color": "czarna", "manual_check": "auto" } }
```

resolves to:

```json
{ "format": "format:a5", "quantity": "<per ladder entry>",
  "components": [
    { "role": "cover",    "pages": 4,  "colors": "4/4", "material": "material:cover_mat_250" },
    { "role": "interior", "pages": 20, "colors": "4/4", "material": "material:interior_mat_130" } ],
  "operations": [
    { "operation": "operation:spiral_binding", "params": { "material": "material:spiral_black" } },
    { "operation": "operation:cutting",  "params": {} },
    { "operation": "operation:prepress", "params": {} } ],
  "technology_allow": null }
```

No `backing` component here (unlike the §9 golden fixture's spiral notebook) — this product's cover is a single 4-page unit wrapping front and back, so two components suffice.
