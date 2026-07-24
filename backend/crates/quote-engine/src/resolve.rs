//! §4 — resolution: a parameter selection becomes a [`JobSpec`].
//!
//! `JobSpec` is also the normative direct-spec wire format for staff estimating
//! (staff-quoting delta 1); [`Component::machine`] is the staff-only
//! per-component machine pin (delta 2). Templates never produce a pinned
//! machine — no effect sets it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::effect::Effect;
use crate::error::{EngineError, ResolveError, SelectionReason};
use crate::model::{Colors, OptionDef, ParameterKind, PriceModel, ProductTemplate, Technology};

/// A parameter selection: parameter `code` → option `code` (string) for select
/// parameters, number for numeric parameters (§4.1).
pub type Selection = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobSpec {
    /// Format record id.
    pub format: String,
    pub quantity: u32,
    pub components: Vec<Component>,
    pub operations: Vec<OperationInstance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technology_allow: Option<Vec<Technology>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub role: String,
    pub pages: u32,
    pub colors: Colors,
    pub material: String,
    /// Staff-only machine pin (delta 2): restrict costing to this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationInstance {
    pub operation: String,
    #[serde(default)]
    pub params: Map<String, Value>,
}

/// Mutable spec under construction; `Option` fields become required at the
/// completeness check (`E106`).
struct SpecBuilder {
    format: Option<String>,
    quantity: u32,
    components: Vec<CompBuilder>,
    operations: Vec<OperationInstance>,
    technology_allow: Option<Vec<Technology>>,
}

struct CompBuilder {
    role: String,
    pages: Option<u32>,
    colors: Option<Colors>,
    material: Option<String>,
    machine: Option<String>,
}

impl SpecBuilder {
    fn component_mut(&mut self, role: &str) -> Option<&mut CompBuilder> {
        self.components.iter_mut().find(|c| c.role == role)
    }

    fn has_operation(&self, id: &str) -> bool {
        self.operations.iter().any(|o| o.operation == id)
    }
}

/// Resolve a selection into a `JobSpec` at a given quantity (§4.2).
pub fn resolve(
    model: &PriceModel,
    template: &ProductTemplate,
    selection: &Selection,
    qty: u32,
) -> Result<JobSpec, EngineError> {
    reject_unknown_keys(template, selection)?;

    let mut builder = SpecBuilder {
        format: None,
        quantity: qty,
        components: template
            .components
            .iter()
            .map(|role| CompBuilder {
                role: role.clone(),
                pages: None,
                colors: None,
                material: None,
                machine: None,
            })
            .collect(),
        operations: Vec::new(),
        technology_allow: None,
    };

    for effect in &template.base_effects {
        apply_effect(model, &mut builder, effect, None)?;
    }

    for param in &template.parameters {
        match &param.kind {
            ParameterKind::Select { options } => {
                let option = choose_option(&param.code, options, selection)?;
                for effect in &option.effects {
                    apply_effect(model, &mut builder, effect, None)?;
                }
            }
            ParameterKind::Numeric {
                input,
                default,
                effects,
            } => {
                let value = choose_numeric(&param.code, input, *default, selection)?;
                for effect in effects {
                    apply_effect(model, &mut builder, effect, Some(value))?;
                }
            }
        }
    }

    finish(builder)
}

fn reject_unknown_keys(
    template: &ProductTemplate,
    selection: &Selection,
) -> Result<(), EngineError> {
    for key in selection.keys() {
        if !template.parameters.iter().any(|p| &p.code == key) {
            return Err(EngineError::InvalidSelection {
                parameter: key.clone(),
                reason: SelectionReason::UnknownParameter,
            });
        }
    }
    Ok(())
}

fn choose_option<'a>(
    code: &str,
    options: &'a [OptionDef],
    selection: &Selection,
) -> Result<&'a OptionDef, EngineError> {
    let invalid = |reason| EngineError::InvalidSelection {
        parameter: code.to_string(),
        reason,
    };

    let option = match selection.get(code) {
        Some(value) => {
            let chosen = value
                .as_str()
                .ok_or_else(|| invalid(SelectionReason::UnknownOption))?;
            options
                .iter()
                .find(|o| o.code == chosen)
                .ok_or_else(|| invalid(SelectionReason::UnknownOption))?
        }
        None => options
            .iter()
            .find(|o| o.is_default)
            .ok_or_else(|| invalid(SelectionReason::MissingNoDefault))?,
    };

    // §4.1 case 4: enforced server-side even for a defaulted option — the
    // portal's greyed-out UI is not a security boundary.
    if !option.available {
        return Err(invalid(SelectionReason::OptionUnavailable));
    }
    Ok(option)
}

fn choose_numeric(
    code: &str,
    input: &crate::model::NumericInput,
    default: Option<u32>,
    selection: &Selection,
) -> Result<u32, EngineError> {
    let invalid = |reason| EngineError::InvalidSelection {
        parameter: code.to_string(),
        reason,
    };
    let in_range =
        |v: u32| v >= input.min && v <= input.max && (v - input.min).is_multiple_of(input.step);

    match selection.get(code) {
        Some(value) => {
            let v = value
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| invalid(SelectionReason::OutOfRange))?;
            if !in_range(v) {
                return Err(invalid(SelectionReason::OutOfRange));
            }
            Ok(v)
        }
        None => {
            let d = default.ok_or_else(|| invalid(SelectionReason::MissingNoDefault))?;
            // A bad default is the template's fault (E108), not the caller's.
            if !in_range(d) {
                return Err(ResolveError::BadNumericDefault(code.to_string()).into());
            }
            Ok(d)
        }
    }
}

fn apply_effect(
    model: &PriceModel,
    builder: &mut SpecBuilder,
    effect: &Effect,
    input: Option<u32>,
) -> Result<(), ResolveError> {
    match effect {
        Effect::SetFormat { format } => {
            if !model.formats.contains_key(format) {
                return Err(ResolveError::DanglingReference(format.clone()));
            }
            builder.format = Some(format.clone());
        }
        Effect::SetPages { target, value } => {
            let pages = value
                .resolve(input)
                .ok_or(ResolveError::InputOutsideNumeric)?;
            builder
                .component_mut(target)
                .ok_or_else(|| ResolveError::UnknownRole(target.clone()))?
                .pages = Some(pages);
        }
        Effect::SetColors { target, value } => {
            builder
                .component_mut(target)
                .ok_or_else(|| ResolveError::UnknownRole(target.clone()))?
                .colors = Some(*value);
        }
        Effect::SetMaterial { target, material } => {
            require_per_sheet(model, material)?;
            builder
                .component_mut(target)
                .ok_or_else(|| ResolveError::UnknownRole(target.clone()))?
                .material = Some(material.clone());
        }
        Effect::AddOperation { operation, params } => {
            if !model.operations.contains_key(operation) {
                return Err(ResolveError::DanglingReference(operation.clone()));
            }
            if builder.has_operation(operation) {
                return Err(ResolveError::DuplicateOperation(operation.clone()));
            }
            builder.operations.push(OperationInstance {
                operation: operation.clone(),
                params: params.clone(),
            });
        }
        Effect::SetOpParam {
            operation,
            param,
            value,
        } => {
            let instance = builder
                .operations
                .iter_mut()
                .find(|o| &o.operation == operation)
                .ok_or_else(|| ResolveError::OpParamOnAbsentOperation(operation.clone()))?;
            instance.params.insert(param.clone(), value.clone());
        }
        Effect::AddComponent {
            role,
            pages,
            colors,
            material,
        } => {
            if builder.components.iter().any(|c| &c.role == role) {
                return Err(ResolveError::ComponentExists(role.clone()));
            }
            require_per_sheet(model, material)?;
            builder.components.push(CompBuilder {
                role: role.clone(),
                pages: Some(*pages),
                colors: Some(*colors),
                material: Some(material.clone()),
                machine: None,
            });
        }
        Effect::ConstrainTechnology { allow } => {
            let next = match &builder.technology_allow {
                None => allow.clone(),
                Some(prev) => {
                    let inter: Vec<Technology> =
                        prev.iter().copied().filter(|t| allow.contains(t)).collect();
                    if inter.is_empty() {
                        return Err(ResolveError::EmptyTechnologyAllow);
                    }
                    inter
                }
            };
            builder.technology_allow = Some(next);
        }
    }
    Ok(())
}

/// A component's material must be `per_sheet`-priced — imposition needs a sheet
/// size (`E107`).
fn require_per_sheet(model: &PriceModel, material: &str) -> Result<(), ResolveError> {
    match model.materials.get(material) {
        Some(m) if m.pricing.sheet_size_mm().is_some() => Ok(()),
        _ => Err(ResolveError::DanglingReference(material.to_string())),
    }
}

/// Completeness check (`E106`) and conversion to the immutable `JobSpec`.
fn finish(builder: SpecBuilder) -> Result<JobSpec, EngineError> {
    let format = builder
        .format
        .ok_or_else(|| ResolveError::IncompleteSpec("format not set".into()))?;
    if builder.quantity < 1 {
        return Err(ResolveError::IncompleteSpec("quantity must be >= 1".into()).into());
    }

    let mut components = Vec::with_capacity(builder.components.len());
    for c in builder.components {
        let pages = c
            .pages
            .filter(|p| *p >= 1)
            .ok_or_else(|| ResolveError::IncompleteSpec(format!("{}: pages", c.role)))?;
        let colors = c
            .colors
            .ok_or_else(|| ResolveError::IncompleteSpec(format!("{}: colors", c.role)))?;
        let material = c
            .material
            .ok_or_else(|| ResolveError::IncompleteSpec(format!("{}: material", c.role)))?;
        components.push(Component {
            role: c.role,
            pages,
            colors,
            material,
            machine: c.machine,
        });
    }

    Ok(JobSpec {
        format,
        quantity: builder.quantity,
        components,
        operations: builder.operations,
        technology_allow: builder.technology_allow,
    })
}
