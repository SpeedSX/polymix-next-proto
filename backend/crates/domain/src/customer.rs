use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Address {
    pub street: Option<String>,
    pub zip: Option<String>,
    pub city: Option<String>,
    #[validate(length(equal = 2, code = "invalid_country_code"))]
    pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: String,
    pub name: String,
    pub contact_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub address: Option<Address>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewCustomer {
    #[validate(length(min = 1, code = "required"))]
    pub name: String,
    pub contact_name: Option<String>,
    #[validate(email(code = "invalid_email"))]
    pub email: Option<String>,
    pub phone: Option<String>,
    #[validate(nested)]
    pub address: Option<Address>,
    pub notes: Option<String>,
}

impl NewCustomer {
    pub fn validate_domain(&self) -> Result<(), DomainError> {
        self.validate().map_err(DomainError::from)
    }
}

#[derive(Debug, Clone)]
pub struct ListQuery {
    pub page: u32,
    pub limit: u32,
    pub sort: String,
    /// Full-text filter (M3). When present, results are ranked by BM25
    /// score and `sort` is ignored, per PLAN.md's list-parameters contract.
    pub q: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
}

#[async_trait::async_trait]
pub trait CustomerRepo: Send + Sync {
    async fn list(&self, query: ListQuery) -> Result<Paged<Customer>, DomainError>;
    async fn get(&self, id: &str) -> Result<Option<Customer>, DomainError>;
    async fn create(&self, data: NewCustomer) -> Result<Customer, DomainError>;
    async fn update(&self, id: &str, data: NewCustomer) -> Result<Customer, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
    /// Top BM25-ranked hits for the global omnibox (M3). `q` is assumed
    /// non-empty — callers filter that out before calling.
    async fn search(&self, q: &str, limit: u32) -> Result<Vec<crate::SearchHit>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_name_is_invalid() {
        let data = NewCustomer {
            name: String::new(),
            contact_name: None,
            email: None,
            phone: None,
            address: None,
            notes: None,
        };

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("name"));
    }

    #[test]
    fn malformed_email_is_invalid() {
        let data = NewCustomer {
            name: "Adamant Print GmbH".to_string(),
            contact_name: None,
            email: Some("not-an-email".to_string()),
            phone: None,
            address: None,
            notes: None,
        };

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("email"));
    }

    #[test]
    fn short_country_code_is_invalid() {
        let data = NewCustomer {
            name: "Adamant Print GmbH".to_string(),
            contact_name: None,
            email: None,
            phone: None,
            address: Some(Address {
                street: None,
                zip: None,
                city: None,
                country: Some("Germany".to_string()),
            }),
            notes: None,
        };

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("address.country"));
    }

    #[test]
    fn valid_customer_passes() {
        let data = NewCustomer {
            name: "Adamant Print GmbH".to_string(),
            contact_name: Some("Ada Lovelace".to_string()),
            email: Some("ada@adamant-print.example".to_string()),
            phone: None,
            address: Some(Address {
                street: Some("Hauptstr. 1".to_string()),
                zip: Some("10115".to_string()),
                city: Some("Berlin".to_string()),
                country: Some("DE".to_string()),
            }),
            notes: None,
        };

        assert!(data.validate_domain().is_ok());
    }
}
