//! §10 tests 2, 3, 4, 5, 9 and the pricing-error codes (E201/E202/E204).

use quote_engine::{
    CompatibilityRule, Dataset, EngineError, JobSpec, PriceError, PriceModel, PricingPolicy,
    ResolveError, Selection, SelectionReason, price_job, resolve, ups, violations,
};
use serde_json::{Value, json};

// ---- shared catalog -------------------------------------------------------

/// A small catalog covering every basis and capability path, with no template.
fn base_dataset() -> Value {
    json!({
        "pricing_policies": [{
            "id": "pricing_policy:p", "currency": "EUR",
            "margin_bands": [{ "min_qty": 1, "multiplier_bp": 10000 }],
            "rounding": { "step_minor": 1, "mode": "up" }, "min_price_minor": 0
        }],
        "formats": [
            { "id": "format:a5", "name": "A5", "trim_mm": [148, 210] },
            { "id": "format:huge", "name": "Huge", "trim_mm": [2000, 3000] }
        ],
        "materials": [
            { "id": "material:paper", "name": "Paper", "kind": "paper",
              "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 40000 },
              "printable": { "grammage_gsm": 80 } },
            { "id": "material:board", "name": "Board", "kind": "board",
              "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 0 },
              "printable": { "grammage_gsm": 500 } },
            { "id": "material:mat_item", "name": "Item mat", "kind": "x",
              "pricing": { "basis": "per_item", "price_micro": 1000 } },
            { "id": "material:mat_sheet", "name": "Sheet mat", "kind": "x",
              "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 1000 } },
            { "id": "material:mat_cm", "name": "Cm mat", "kind": "x",
              "pricing": { "basis": "per_cm", "price_micro": 1000 } },
            { "id": "material:mat_m2", "name": "M2 mat", "kind": "x",
              "pricing": { "basis": "per_m2", "price_micro": 1000 } }
        ],
        "machines": [
            { "id": "machine:digi", "name": "Digi", "technology": "digital",
              "sheet_size_mm": [320, 450], "duplex": true, "max_grammage_gsm": 350,
              "setup_micro": 2000000, "click_mono_micro": 8000, "click_color_micro": 60000,
              "waste_fixed_sheets": 10, "waste_percent": 2 }
        ],
        "operations": [
            { "id": "operation:cut", "setup_micro": 100000, "unit_basis": "per_item", "unit_price_micro": 2000 },
            { "id": "operation:op_item", "setup_micro": 100000, "unit_basis": "per_item", "unit_price_micro": 2000 },
            { "id": "operation:op_sheet", "setup_micro": 100000, "unit_basis": "per_sheet", "unit_price_micro": 2000 },
            { "id": "operation:op_cm", "setup_micro": 100000, "unit_basis": "per_cm", "unit_price_micro": 2000 },
            { "id": "operation:op_m2", "setup_micro": 100000, "unit_basis": "per_m2", "unit_price_micro": 2000 }
        ]
    })
}

fn base_model() -> PriceModel {
    let ds: Dataset = serde_json::from_value(base_dataset()).unwrap();
    PriceModel::from_dataset(ds)
}

/// Build a model whose catalog is [`base_dataset`] plus one template.
fn model_with_template(template: Value) -> PriceModel {
    let mut ds = base_dataset();
    ds["templates"] = json!([template]);
    let ds: Dataset = serde_json::from_value(ds).unwrap();
    PriceModel::from_dataset(ds)
}

fn template(components: Value, base_effects: Value, parameters: Value) -> Value {
    json!({
        "id": "product_template:t", "slug": "t", "name": { "en": "T" },
        "components": components, "pricing_policy": "pricing_policy:p",
        "quantities": [100], "custom_quantity": { "min": 1, "max": 5000 },
        "base_effects": base_effects, "parameters": parameters
    })
}

fn resolve_broken(components: Value, base_effects: Value, parameters: Value) -> EngineError {
    let model = model_with_template(template(components, base_effects, parameters));
    let t = model.template_by_slug("t").unwrap().clone();
    let selection = Selection::new();
    resolve(&model, &t, &selection, 100).expect_err("expected resolution to fail")
}

// ---- §10 test 2: ups reference table --------------------------------------

#[test]
fn ups_reference_values_on_sra3() {
    let sra3 = [320, 450];
    assert_eq!(ups(sra3, [105, 148]), 8, "A6");
    assert_eq!(ups(sra3, [99, 210]), 6, "DL");
    assert_eq!(ups(sra3, [148, 210]), 4, "A5");
    assert_eq!(ups(sra3, [210, 297]), 2, "A4");
}

// ---- §10 test 3: resolution error codes E101–E110 -------------------------

#[test]
fn e101_unknown_target_role() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_pages", "target": "ghost", "value": 2 }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::UnknownRole(_))
    ));
}

#[test]
fn e102_duplicate_add_operation() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([
            { "kind": "add_operation", "operation": "operation:cut" },
            { "kind": "add_operation", "operation": "operation:cut" }
        ]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::DuplicateOperation(_))
    ));
}

#[test]
fn e103_set_op_param_on_absent_operation() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_op_param", "operation": "operation:cut", "param": "material", "value": "material:mat_item" }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::OpParamOnAbsentOperation(_))
    ));
}

#[test]
fn e104_add_component_on_existing_role() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "add_component", "role": "cover", "pages": 2, "colors": "0/0", "material": "material:paper" }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::ComponentExists(_))
    ));
}

#[test]
fn e105_empty_technology_allow() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([
            { "kind": "constrain_technology", "allow": ["digital"] },
            { "kind": "constrain_technology", "allow": ["offset"] }
        ]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::EmptyTechnologyAllow)
    ));
}

#[test]
fn e106_incomplete_spec() {
    // Format set, but cover never gets pages/colors/material.
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_format", "format": "format:a5" }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::IncompleteSpec(_))
    ));
}

#[test]
fn e107_dangling_record_reference() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_format", "format": "format:nope" }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::DanglingReference(_))
    ));
}

#[test]
fn e108_numeric_default_violates_input() {
    // default 25 violates step 10 from min 20; parameter omitted from selection.
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_format", "format": "format:a5" }]),
        json!([{ "code": "n", "kind": "numeric", "input": { "min": 20, "max": 200, "step": 10 }, "default": 25, "effects": [] }]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::BadNumericDefault(_))
    ));
}

#[test]
fn e110_input_outside_numeric_parameter() {
    let err = resolve_broken(
        json!(["cover"]),
        json!([{ "kind": "set_pages", "target": "cover", "value": { "$input": {} } }]),
        json!([]),
    );
    assert!(matches!(
        err,
        EngineError::Resolve(ResolveError::InputOutsideNumeric)
    ));
}

// ---- §10 test 4: rule evaluation ------------------------------------------

fn rule(json_str: &str) -> CompatibilityRule {
    serde_json::from_str(json_str).unwrap()
}

fn spec(interior_pages: u32, with_spiral: bool) -> JobSpec {
    let operations = if with_spiral {
        json!([{ "operation": "operation:spiral_binding" }])
    } else {
        json!([])
    };
    serde_json::from_value(json!({
        "format": "format:a5", "quantity": 100,
        "components": [{ "role": "interior", "pages": interior_pages, "colors": "4/4", "material": "material:paper" }],
        "operations": operations
    }))
    .unwrap()
}

#[test]
fn rule_violated_when_condition_holds_and_require_fails() {
    let r = rule(
        r#"{"id":"r","template":"t","when":{"op_present":"operation:spiral_binding"},
            "require":{"attr":"component:interior.pages","op":"gte","value":20},"message":{}}"#,
    );
    assert_eq!(
        violations(std::slice::from_ref(&r), &spec(100, true)).len(),
        0,
        "satisfied"
    );
    assert_eq!(
        violations(std::slice::from_ref(&r), &spec(10, true)).len(),
        1,
        "violated"
    );
    // `when` false (no spiral op) -> rule inactive even though pages < 20.
    assert_eq!(
        violations(std::slice::from_ref(&r), &spec(10, false)).len(),
        0,
        "when false"
    );
}

#[test]
fn rule_when_absent_is_always_active_and_missing_attr_is_false() {
    // No `when`; require an attr on a component that does not exist -> false -> violated.
    let r = rule(
        r#"{"id":"r2","template":"t","require":{"attr":"component:ghost.pages","op":"gte","value":1},"message":{}}"#,
    );
    assert_eq!(
        violations(std::slice::from_ref(&r), &spec(100, true)).len(),
        1
    );
}

// ---- §10 test 5: selection validation -------------------------------------

fn demo_select_template() -> Value {
    template(
        json!(["cover"]),
        json!([{ "kind": "set_format", "format": "format:a5" }]),
        json!([{ "code": "fmt", "kind": "select", "options": [
            { "code": "a", "is_default": true, "effects": [{ "kind": "set_colors", "target": "cover", "value": "4/0" }] },
            { "code": "b", "effects": [{ "kind": "set_colors", "target": "cover", "value": "1/1" }] }
        ]},
        { "code": "mat", "kind": "select", "options": [
            { "code": "p", "is_default": true, "effects": [{ "kind": "set_material", "target": "cover", "material": "material:paper" }] }
        ]},
        { "code": "pg", "kind": "numeric", "input": { "min": 2, "max": 200, "step": 2 }, "default": 4,
          "effects": [{ "kind": "set_pages", "target": "cover", "value": { "$input": { "mul": 1 } } }] }]),
    )
}

fn resolve_selection(sel: &str, qty: u32) -> Result<JobSpec, EngineError> {
    let model = model_with_template(demo_select_template());
    let t = model.template_by_slug("t").unwrap().clone();
    let selection: Selection = serde_json::from_str(sel).unwrap();
    resolve(&model, &t, &selection, qty)
}

#[test]
fn unknown_key_is_rejected() {
    let err = resolve_selection(r#"{"nope":"x"}"#, 100).unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidSelection {
            reason: SelectionReason::UnknownParameter,
            ..
        }
    ));
}

#[test]
fn missing_select_and_numeric_take_their_defaults() {
    let spec = resolve_selection("{}", 100).unwrap();
    let cover = &spec.components[0];
    assert_eq!(
        cover.colors,
        quote_engine::Colors { front: 4, back: 0 },
        "select default a"
    );
    assert_eq!(cover.pages, 4, "numeric default");
}

#[test]
fn unknown_option_is_rejected() {
    let err = resolve_selection(r#"{"fmt":"zzz"}"#, 100).unwrap_err();
    assert!(matches!(
        err,
        EngineError::InvalidSelection {
            reason: SelectionReason::UnknownOption,
            ..
        }
    ));
}

#[test]
fn numeric_out_of_range_is_rejected() {
    let err = resolve_selection(r#"{"pg":3}"#, 100).unwrap_err(); // step 2 from min 2 -> 3 invalid
    assert!(matches!(
        err,
        EngineError::InvalidSelection {
            reason: SelectionReason::OutOfRange,
            ..
        }
    ));
}

#[test]
fn unavailable_option_is_rejected_both_explicitly_and_via_default() {
    let tmpl = template(
        json!(["cover"]),
        json!([{ "kind": "set_format", "format": "format:a5" }]),
        json!([{ "code": "x", "kind": "select", "options": [
            { "code": "a", "is_default": true, "available": false,
              "effects": [{ "kind": "set_colors", "target": "cover", "value": "4/0" }] }
        ]}]),
    );
    let model = model_with_template(tmpl);
    let t = model.template_by_slug("t").unwrap().clone();

    let via_default = resolve(&model, &t, &Selection::new(), 100).unwrap_err();
    let explicit: Selection = serde_json::from_str(r#"{"x":"a"}"#).unwrap();
    let explicit = resolve(&model, &t, &explicit, 100).unwrap_err();

    for err in [via_default, explicit] {
        assert!(matches!(
            err,
            EngineError::InvalidSelection {
                reason: SelectionReason::OptionUnavailable,
                ..
            }
        ));
    }
}

// ---- pricing error codes E201 / E202 / E204 -------------------------------

fn price_spec(job: Value) -> Result<quote_engine::Breakdown, EngineError> {
    let model = base_model();
    let policy: PricingPolicy = model.pricing_policies["pricing_policy:p"].clone();
    let job: JobSpec = serde_json::from_value(job).unwrap();
    price_job(&model, &policy, &job, None)
}

#[test]
fn e201_no_capable_machine() {
    // board_500 is over the only machine's 350 g cap; printed -> no capable machine.
    let err = price_spec(json!({
        "format": "format:a5", "quantity": 100,
        "components": [{ "role": "cover", "pages": 2, "colors": "4/0", "material": "material:board" }],
        "operations": []
    }))
    .unwrap_err();
    assert!(matches!(
        err,
        EngineError::Price(PriceError::NoCapableMachine(_))
    ));
}

#[test]
fn e202_item_larger_than_sheet() {
    let err = price_spec(json!({
        "format": "format:huge", "quantity": 100,
        "components": [{ "role": "cover", "pages": 2, "colors": "0/0", "material": "material:paper" }],
        "operations": []
    }))
    .unwrap_err();
    assert!(matches!(
        err,
        EngineError::Price(PriceError::ItemLargerThanSheet(_))
    ));
}

#[test]
fn e204_material_basis_mismatch() {
    // op_item is per_item; mat_cm is per_cm.
    let err = price_spec(json!({
        "format": "format:a5", "quantity": 100,
        "components": [{ "role": "backing", "pages": 2, "colors": "0/0", "material": "material:board" }],
        "operations": [{ "operation": "operation:op_item", "params": { "material": "material:mat_cm" } }]
    }))
    .unwrap_err();
    assert!(matches!(
        err,
        EngineError::Price(PriceError::MaterialBasisMismatch(_))
    ));
}

// ---- §10 test 9: units_multiplier -----------------------------------------

const OP_SETUP: i64 = 100000;

/// Operation cost for a single-operation job over an unprinted backing.
fn op_cost(op_id: &str, params: Value) -> Result<i64, EngineError> {
    let bd = price_spec(json!({
        "format": "format:a5", "quantity": 100,
        "components": [{ "role": "backing", "pages": 2, "colors": "0/0", "material": "material:board" }],
        "operations": [{ "operation": op_id, "params": params }]
    }))?;
    Ok(bd.operations[0].cost_micro)
}

#[test]
fn units_multiplier_doubles_unit_term_for_every_basis() {
    let cases = [
        ("operation:op_item", "material:mat_item"),
        ("operation:op_sheet", "material:mat_sheet"),
        ("operation:op_cm", "material:mat_cm"),
        ("operation:op_m2", "material:mat_m2"),
    ];
    for (op, material) in cases {
        let absent = op_cost(op, json!({ "material": material })).unwrap();
        let one = op_cost(op, json!({ "material": material, "units_multiplier": 1 })).unwrap();
        let two = op_cost(op, json!({ "material": material, "units_multiplier": 2 })).unwrap();

        assert_eq!(absent, one, "{op}: absent multiplier behaves as 1");
        assert_eq!(
            two - OP_SETUP,
            2 * (one - OP_SETUP),
            "{op}: units_multiplier 2 doubles the unit term (incl. material), setup unchanged"
        );
    }
}

#[test]
fn units_multiplier_invalid_values_are_e205() {
    for bad in [json!(0), json!(1.5)] {
        let err = op_cost(
            "operation:op_item",
            json!({ "material": "material:mat_item", "units_multiplier": bad }),
        )
        .unwrap_err();
        assert!(
            matches!(err, EngineError::Price(PriceError::InvalidReservedParam(_))),
            "units_multiplier {bad} should be E205"
        );
    }
}
