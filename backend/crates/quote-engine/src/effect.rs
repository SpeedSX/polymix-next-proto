//! §3 — the effect vocabulary: the closed, engine-owned bridge between
//! tenant-defined options and the [`JobSpec`](crate::resolve::JobSpec).
//!
//! An unknown `kind` fails template deserialization — the enum is closed by
//! design.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::model::{Colors, Technology};

/// `ColorsSpec` in the spec — the `"F/B"` string form of [`Colors`].
pub type ColorsSpec = Colors;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    SetFormat {
        format: String,
    },
    SetPages {
        target: String,
        value: NumOrInput,
    },
    SetColors {
        target: String,
        value: ColorsSpec,
    },
    SetMaterial {
        target: String,
        material: String,
    },
    AddOperation {
        operation: String,
        #[serde(default)]
        params: Map<String, Value>,
    },
    SetOpParam {
        operation: String,
        param: String,
        value: Value,
    },
    AddComponent {
        role: String,
        pages: u32,
        colors: ColorsSpec,
        material: String,
    },
    ConstrainTechnology {
        allow: Vec<Technology>,
    },
}

/// A numeric-effect field (§4.3): either a literal, or a linear function of the
/// numeric parameter's input value. Enforcing that the `$input` form only
/// appears inside a numeric parameter's effects (`E110`) is contextual and
/// lives in resolution, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumOrInput {
    Literal(u32),
    Input {
        #[serde(rename = "$input")]
        input: InputExpr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputExpr {
    #[serde(default = "one")]
    pub mul: u32,
    #[serde(default)]
    pub add: u32,
}

fn one() -> u32 {
    1
}

impl NumOrInput {
    /// Resolve against a numeric parameter's `input` value. `None` for the
    /// `Input` form signals the value was reached outside a numeric parameter
    /// (`E110`), which the caller turns into a resolution error.
    pub fn resolve(&self, input: Option<u32>) -> Option<u32> {
        match self {
            Self::Literal(v) => Some(*v),
            Self::Input { input: expr } => input.map(|i| i * expr.mul + expr.add),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pages_accepts_literal_and_input_forms() {
        let literal: Effect =
            serde_json::from_str(r#"{"kind":"set_pages","target":"interior","value":100}"#)
                .unwrap();
        assert!(matches!(
            literal,
            Effect::SetPages {
                value: NumOrInput::Literal(100),
                ..
            }
        ));

        let input: Effect = serde_json::from_str(
            r#"{"kind":"set_pages","target":"interior","value":{"$input":{"mul":2}}}"#,
        )
        .unwrap();
        let Effect::SetPages { value, .. } = input else {
            panic!("expected set_pages")
        };
        assert_eq!(value.resolve(Some(50)), Some(100));
        assert_eq!(value.resolve(None), None); // $input outside numeric ctx -> E110
    }

    #[test]
    fn input_expr_defaults_mul_1_add_0() {
        let value: NumOrInput = serde_json::from_str(r#"{"$input":{}}"#).unwrap();
        assert_eq!(value.resolve(Some(7)), Some(7));
    }

    #[test]
    fn unknown_effect_kind_is_rejected() {
        assert!(serde_json::from_str::<Effect>(r#"{"kind":"teleport","x":1}"#).is_err());
    }

    #[test]
    fn colors_effect_uses_string_form() {
        let e: Effect =
            serde_json::from_str(r#"{"kind":"set_colors","target":"cover","value":"4/0"}"#)
                .unwrap();
        assert!(matches!(
            e,
            Effect::SetColors {
                value: Colors { front: 4, back: 0 },
                ..
            }
        ));
    }
}
