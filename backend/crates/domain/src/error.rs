use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
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
