//! §5 — compatibility rules: a JSON AST evaluated against the resolved
//! `JobSpec`. No text DSL. A rule is *violated* iff its `when` holds (absent =
//! always) and its `require` does not.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::RuleViolation;
use crate::model::I18n;
use crate::resolve::JobSpec;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityRule {
    pub id: String,
    pub template: String,
    /// Optional; absent = always active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    pub require: Condition,
    #[serde(default)]
    pub message: I18n,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    All {
        all: Vec<Condition>,
    },
    Any {
        any: Vec<Condition>,
    },
    Not {
        not: Box<Condition>,
    },
    OpPresent {
        op_present: String,
    },
    Attr {
        attr: String,
        op: CmpOp,
        value: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Eq,
    Ne,
    Gte,
    Lte,
    In,
}

/// Every rule violated by `spec`, localized (§5: all violations returned
/// together, not just the first).
pub fn violations(rules: &[CompatibilityRule], spec: &JobSpec) -> Vec<RuleViolation> {
    rules
        .iter()
        .filter(|r| is_violated(r, spec))
        .map(|r| RuleViolation {
            rule_id: r.id.clone(),
            message: r.message.clone(),
        })
        .collect()
}

fn is_violated(rule: &CompatibilityRule, spec: &JobSpec) -> bool {
    let when = rule.when.as_ref().map(|c| eval(c, spec)).unwrap_or(true);
    when && !eval(&rule.require, spec)
}

fn eval(condition: &Condition, spec: &JobSpec) -> bool {
    match condition {
        Condition::All { all } => all.iter().all(|c| eval(c, spec)),
        Condition::Any { any } => any.iter().any(|c| eval(c, spec)),
        Condition::Not { not } => !eval(not, spec),
        Condition::OpPresent { op_present } => {
            spec.operations.iter().any(|o| &o.operation == op_present)
        }
        // An attr on a missing component or unset attribute evaluates false (§5).
        Condition::Attr { attr, op, value } => match eval_attr(attr, spec) {
            Some(left) => cmp(*op, &left, value),
            None => false,
        },
    }
}

fn cmp(op: CmpOp, left: &Value, right: &Value) -> bool {
    match op {
        CmpOp::Eq => values_equal(left, right),
        CmpOp::Ne => !values_equal(left, right),
        CmpOp::Gte => numeric_pair(left, right).is_some_and(|(l, r)| l >= r),
        CmpOp::Lte => numeric_pair(left, right).is_some_and(|(l, r)| l <= r),
        CmpOp::In => right
            .as_array()
            .is_some_and(|arr| arr.iter().any(|e| values_equal(e, left))),
    }
}

/// Numeric equality when both are numbers (so `100 == 100` regardless of JSON
/// int/float spelling); otherwise structural equality (covers record-id and
/// enum-code strings).
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a.as_i64(), b.as_i64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn numeric_pair(a: &Value, b: &Value) -> Option<(i64, i64)> {
    Some((a.as_i64()?, b.as_i64()?))
}

/// Resolve a §5 path against the spec; `None` when the component or attribute
/// is absent.
fn eval_attr(path: &str, spec: &JobSpec) -> Option<Value> {
    match path {
        "quantity" => Some(Value::from(spec.quantity)),
        "format" => Some(Value::from(spec.format.clone())),
        _ => {
            let rest = path.strip_prefix("component:")?;
            let (role, attr) = rest.split_once('.')?;
            let component = spec.components.iter().find(|c| c.role == role)?;
            match attr {
                "pages" => Some(Value::from(component.pages)),
                "material" => Some(Value::from(component.material.clone())),
                "colors.front" => Some(Value::from(component.colors.front)),
                "colors.back" => Some(Value::from(component.colors.back)),
                _ => None,
            }
        }
    }
}
