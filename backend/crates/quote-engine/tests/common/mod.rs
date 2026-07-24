#![allow(dead_code)] // shared across test binaries; not every binary uses every helper

use quote_engine::{Dataset, PriceModel, ProductTemplate, Selection};

pub fn demo_model() -> PriceModel {
    let ds: Dataset = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/demo.json"
    )))
    .expect("demo.json parses");
    PriceModel::from_dataset(ds)
}

pub fn demo_template(model: &PriceModel) -> ProductTemplate {
    model
        .template_by_slug("spiral-notebook")
        .expect("spiral-notebook template")
        .clone()
}

pub fn expected() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/expected.json"
    )))
    .expect("expected.json parses")
}

/// The §9.2 golden selection.
pub fn golden_selection() -> Selection {
    serde_json::from_str(
        r#"{
            "format": "a5", "printing": "4_0", "sheets": "50",
            "cover_stock": "gloss300", "interior_stock": "offset80",
            "spiral_color": "black", "lamination": "none", "backing": "yes"
        }"#,
    )
    .expect("golden selection parses")
}
