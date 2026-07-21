# Instant Quote Engine — Customer Portal Design

How a drukomat.pl-style product configurator returns a full price ladder (50 → 2000 copies) in well under a second for every parameter change, and how we build it on our stack.

## Why precalculation looks impossible — and why it doesn't matter

The intuition is correct: ~10 parameters × up to 20 variants each is up to `20^10 ≈ 10^13` combinations, times ~20 quantity tiers. No one stores that table. **The prices are not looked up — they are computed per request.**

The insight is that parameter combinations don't have *prices*; they have *cost consequences*. Each parameter maps onto a handful of physical cost drivers (paper consumed, machine time, clicks, finishing operations, setup work), and the price is arithmetic over those drivers. A quote is a few hundred floating-point operations — microseconds in any language, let alone Rust. Twenty quantity tiers means running the same function twenty times. The response time of such an endpoint is dominated by network latency, not computation; the ~1 s you observe on drukomat is almost certainly network + an unoptimized stack, not heavy math.

So the design problem is not "how to compute fast" but "how to model printing costs as data a tenant can maintain."

## How the price of a print job forms

For any configuration + quantity, the engine derives a **production plan** and costs it:

1. **Derive physical job from parameters.** Format, page count, cover/interior material, colors, finishing (e.g. spiral binding, lamination) come straight from the dropdowns.
2. **Imposition.** How many items fit on a press sheet ("ups"): A5 on SRA3 = 4 ups. Sheets needed = ⌈qty × pages / (ups × sides)⌉ + waste allowance (make-ready spoilage, typically a fixed count + percentage).
3. **Technology selection.** Cost the job on every capable technology — digital (cost ≈ linear in clicks, near-zero setup) and offset (high setup: plates + make-ready, low per-sheet run cost) — and take the cheapest. This is why real price-per-unit curves have visible breakpoints: below ~300 copies digital wins, above it offset takes over.
4. **Cost components.**
   - *Setup (fixed per job):* prepress, plates (offset), machine make-ready, finishing setup (e.g. spiral machine changeover).
   - *Materials (per sheet):* material price by paper type/weight/format; spirals, boxes.
   - *Run (per sheet or per click):* machine hourly rate × time, or click price for digital.
   - *Finishing (setup + per unit):* cutting, binding, laminating — each operation is `setup + qty × unit_cost`.
5. **Price = (Σ costs) × margin**, with margin possibly varying by product/qty band, rounded per pricing policy.

Fixed setup amortized over quantity is what produces the falling unit price the portal shows — it emerges from the model, nothing is hand-tuned per tier.

### Worked example — spiral notebook, A5, 50 interior leaves, 4/0 cover, qty 100

| Component | Derivation | Cost |
|---|---|---:|
| Interior sheets | 100 × 50 leaves / 4 ups (SRA3, duplex) = 1 250 sheets + 3 % waste ≈ 1 288 | 1 288 × €0.04 (80 g offset) = €51.52 |
| Interior print (digital) | 2 576 mono clicks × €0.008 | €20.61 |
| Cover | 100 / 4 ups = 25 sheets 300 g + waste ≈ 27; 27 color clicks × €0.06 | 27 × €0.12 + €1.62 = €4.86 |
| Cutting | setup €3 + 100 × €0.01 | €4.00 |
| Spiral binding | setup €5 + 100 × €0.18 (spiral + labor) | €23.00 |
| Prepress | fixed | €4.00 |
| **Cost** | | **€107.99** |
| **Price** (margin ×1.6, rounded) | | **€172.90** → €1.73/pc |

Run the same function at qty 50, 100, …, 2000 (and any custom qty) — that's the ladder. Fixed setup amortized over more copies is what makes the unit price fall across the ladder; a technology breakpoint appears wherever a component's cheapest press flips between digital and offset as volume grows.

> **Illustrative only.** The numbers above use rough per-unit figures to show the *shape* of the calculation. The normative worked example — the same product with exact integer µ-unit arithmetic, the real margin bands, and per-component technology choice — is `quote-engine-spec.md §9` (the golden fixture). There, on the demo price model, the cover runs digital and the interior runs offset at *both* qty 100 and 1000, so the falling unit price is pure setup amortization rather than an interior digital→offset switch. Where these two docs disagree on any number, the spec wins.

## Data model (per tenant, SurrealDB)

Everything the formula needs is tenant-editable data — this *is* the MIS part of the product:

- `product_template` — the configurator definition: ordered parameters, each with options (`format: A4|A5|A6`, `interior_stock: …`), plus which quantity ladder to show.
- `option_effect` — what each option does to the job: sets a material reference, adds a finishing operation, multiplies page count, constrains technology. Detailed design with worked templates: `docs/product-configuration.md`.
- `material` — one catalog for every material family (papers, boards, films, spiral wire, foils, boxes): a UI `kind` taxonomy, a pricing model (per sheet / m² / cm / item), `printable` substrate attributes (grammage) for anything a press can print on, and opaque `attrs` for the rest.
- `technology` / `machine` — setup cost, run cost per sheet or click price, max format, capability flags (duplex, max grammage).
- `operation` — finishing ops: setup cost, unit cost, unit basis (per item / per sheet / per cut / per cm of edge / per m²) — geometric bases are fed from the resolved format.
- `pricing_policy` — margin bands, rounding rules, minimum order price.
- `compatibility_rule` — constraint set: `spiral_binding requires pages >= 20`, `lamination excludes cover.material in [uncoated…]`. Used to grey out / filter options in the UI, evaluated the same way on quote to reject invalid configs.

A `pricelist_version` stamp on the whole set makes caching and auditability trivial ("this quote was priced under version 42").

## Engine and API design

- **`crates/quote-engine`** — a pure, deterministic Rust crate: `fn quote(model: &PriceModel, config: &Selection, qty: u32) -> Quote`. No I/O, no async. Property-tested (unit price monotonically non-increasing in qty; every valid config prices without error).
- **In-memory price model.** The tenant's whole cost dataset is small (hundreds of rows). Load it once into an immutable `Arc<PriceModel>` snapshot; a SurrealDB **live query** on the pricing tables invalidates and rebuilds the snapshot when an admin edits a rate. Requests never touch the DB.
- **API:**
  - `GET /portal/products/:slug/schema` — parameters, options, compatibility rules (so the FE can grey out invalid combos without a round trip), default selection.
  - `POST /portal/products/:slug/quote` — `{selection, quantities?: [..]}` → `{ladder: [{qty, total, unit, currency}], pricelist_version, breakdown?}`. Default ladder from the template; custom qty just gets appended. The optional `breakdown` (cost components) is for the authenticated back office, never the public portal. Staff-side quoting (direct-JobSpec estimating beyond templates, quote documents, commercial overrides) is its own design: `docs/staff-quoting.md`.
- **Latency budget:** engine ~µs × 20 tiers; JSON + framework overhead ~1 ms; the rest is network. Sub-100 ms end-to-end is the realistic outcome, comfortably beating the 1 s reference. Response caching (`moka`, keyed on `hash(selection) + pricelist_version`) is a load optimization, not a latency requirement.
- **Portal surface:** the portal is a public (unauthenticated) route group; the tenant is resolved from the domain/slug, not from a JWT — the same middleware seam as the back office, different resolver. Quote requests are rate-limited since they're anonymous.
- **FE behavior:** on each dropdown change, debounce ~150 ms, `POST /quote`, render ladder; TanStack Query with `placeholderData: keepPreviousData` so prices update without flicker. Nothing is calculated client-side — matching the reference site and keeping the pricing model private.

## Alternatives considered

- **Full precalculation** — rejected by arithmetic (`10^13` cells) and by maintenance: one paper-price change would invalidate everything.
- **Matrix price lists** (hand-entered price per config-group × qty grid) — how many small shops actually work. Worth supporting *as an override layer*: if a matrix entry exists for the selection, it wins; otherwise the parametric engine prices it. Cheap to add on top of the same API.
- **Precomputing popular configurations** into a cache warm-up — unnecessary given µs compute; the `moka` cache achieves the same effect lazily.
- **Interpolation/ML over historical quotes** — opaque, unauditable, and print pricing is exactly reproducible from first principles; no reason to approximate.

## Fit into the prototype plan

This lands after the current M6 as **M7 — Portal + instant quote**:

1. `quote-engine` crate with a hardcoded demo price model + property tests (normative implementation spec: `docs/quote-engine-spec.md`).
2. Pricing tables + admin CRUD (minimal Mantine screens) + live-query snapshot rebuild.
3. Public portal route group: product page with configurator, ladder rendering, custom quantity.
4. Compatibility-rule evaluation shared between schema endpoint and quote validation.

Out of scope for the prototype: quote → order conversion with file upload, matrix override layer, multi-technology optimization beyond digital/offset, delivery pricing.
