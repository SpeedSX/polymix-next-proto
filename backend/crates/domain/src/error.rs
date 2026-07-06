use std::collections::HashMap;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DomainError {
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(HashMap<String, String>),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("store error: {0}")]
    Store(String),
}

impl From<validator::ValidationErrors> for DomainError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let mut details = HashMap::new();
        flatten_validation_errors("", &errors, &mut details);
        DomainError::Validation(details)
    }
}

/// `ValidationErrors::field_errors` only sees direct fields, dropping errors from
/// `#[validate(nested)]` structs — flatten those into dotted paths (e.g.
/// `address.country`) so nested validation failures still reach the API response.
fn flatten_validation_errors(
    prefix: &str,
    errors: &validator::ValidationErrors,
    out: &mut HashMap<String, String>,
) {
    for (field, kind) in errors.errors() {
        let key = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };
        match kind {
            validator::ValidationErrorsKind::Field(errs) => {
                if let Some(err) = errs.first() {
                    let message = err
                        .message
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| err.code.to_string());
                    out.insert(key, message);
                }
            }
            validator::ValidationErrorsKind::Struct(nested) => {
                flatten_validation_errors(&key, nested, out);
            }
            validator::ValidationErrorsKind::List(list) => {
                for (idx, nested) in list {
                    flatten_validation_errors(&format!("{key}[{idx}]"), nested, out);
                }
            }
        }
    }
}
