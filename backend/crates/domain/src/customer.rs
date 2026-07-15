use std::collections::HashMap;

use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::error::{ConflictReason, DomainError, FieldError};
use crate::money::Money;
use crate::tenant::Tenant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerKind {
    LegalEntity,
    Fop,
    Individual,
}

impl CustomerKind {
    pub const fn code(self) -> u8 {
        match self {
            Self::LegalEntity => 0,
            Self::Fop => 1,
            Self::Individual => 2,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::LegalEntity => "legal_entity",
            Self::Fop => "fop",
            Self::Individual => "individual",
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::LegalEntity),
            1 => Some(Self::Fop),
            2 => Some(Self::Individual),
            _ => None,
        }
    }
}

impl Serialize for CustomerKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for CustomerKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let code = u8::deserialize(deserializer)?;
        Self::from_code(code)
            .ok_or_else(|| D::Error::custom(format!("invalid customer kind code: {code}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerStatus {
    Lead,
    Active,
    Inactive,
    Blocked,
}

impl CustomerStatus {
    pub const fn code(self) -> u8 {
        match self {
            Self::Lead => 0,
            Self::Active => 1,
            Self::Inactive => 2,
            Self::Blocked => 3,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Lead => "lead",
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Blocked => "blocked",
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Lead),
            1 => Some(Self::Active),
            2 => Some(Self::Inactive),
            3 => Some(Self::Blocked),
            _ => None,
        }
    }
}

impl Serialize for CustomerStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for CustomerStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let code = u8::deserialize(deserializer)?;
        Self::from_code(code)
            .ok_or_else(|| D::Error::custom(format!("invalid customer status code: {code}")))
    }
}

/// Whether `status` allows a new order to be created for the customer — a
/// `lead` order is the conversion event (auto-promotes to `active`), so both
/// count. Enforced by the order service at creation.
pub const fn can_order(status: CustomerStatus) -> bool {
    matches!(status, CustomerStatus::Lead | CustomerStatus::Active)
}

/// Customer lifecycle transitions per `docs/customers-crm.md`:
/// `lead -> active`; `active <-> inactive`; either can be `blocked`;
/// `blocked -> active` (unblock). No other move is allowed. Pure so the
/// full transition matrix can be asserted exhaustively in tests.
pub const fn can_transition(current: CustomerStatus, target: CustomerStatus) -> bool {
    use CustomerStatus::*;
    match current {
        Lead => matches!(target, Active),
        Active => matches!(target, Inactive | Blocked),
        Inactive => matches!(target, Active | Blocked),
        Blocked => matches!(target, Active),
    }
}

/// `validate_transition` is the `DomainError`-producing wrapper `CustomerRepo`
/// implementations call from `set_status`, mirroring `order::validate_transition`.
pub fn validate_transition(
    current: CustomerStatus,
    target: CustomerStatus,
) -> Result<(), DomainError> {
    if can_transition(current, target) {
        Ok(())
    } else {
        Err(DomainError::Conflict(
            ConflictReason::CustomerStatusTransition {
                from: current,
                to: target,
            },
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Address {
    pub street: Option<String>,
    pub zip: Option<String>,
    pub city: Option<String>,
    #[validate(length(equal = 2, code = "invalid_country_code"))]
    pub country: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Contact {
    #[validate(length(min = 1, code = "required"))]
    pub name: String,
    pub role: Option<String>,
    #[validate(email(code = "invalid_email"))]
    pub email: Option<String>,
    pub phone: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    pub id: String,
    pub number: String,
    pub kind: CustomerKind,
    pub name: String,
    pub legal_name: Option<String>,
    pub edrpou: Option<String>,
    pub tax_id: Option<String>,
    pub vat_ipn: Option<String>,
    pub status: CustomerStatus,
    pub tags: Vec<String>,
    pub industry: Option<String>,
    pub source: Option<String>,
    pub website: Option<String>,
    pub contacts: Vec<Contact>,
    pub legal_address: Option<Address>,
    pub delivery_address: Option<Address>,
    pub payment_terms_days: u16,
    pub credit_limit: Option<Money>,
    pub default_currency: String,
    pub default_discount_bp: u16,
    pub iban: Option<String>,
    pub bank_name: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewCustomer {
    pub kind: CustomerKind,
    #[validate(length(min = 1, code = "required"))]
    pub name: String,
    pub legal_name: Option<String>,
    pub edrpou: Option<String>,
    pub tax_id: Option<String>,
    pub vat_ipn: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub industry: Option<String>,
    pub source: Option<String>,
    pub website: Option<String>,
    #[serde(default)]
    #[validate(nested)]
    pub contacts: Vec<Contact>,
    #[validate(nested)]
    pub legal_address: Option<Address>,
    #[validate(nested)]
    pub delivery_address: Option<Address>,
    #[validate(range(min = 0, max = 365, code = "out_of_range"))]
    pub payment_terms_days: u16,
    #[validate(nested)]
    pub credit_limit: Option<Money>,
    pub default_currency: Option<String>,
    #[validate(range(min = 0, max = 10000, code = "out_of_range"))]
    pub default_discount_bp: u16,
    pub iban: Option<String>,
    pub bank_name: Option<String>,
    pub notes: Option<String>,
    /// Accepted only at creation, and only `0` (lead) or `1` (active) — see
    /// `validate_creation_status`. Ignored entirely on update; status changes
    /// after creation only happen through the dedicated status route.
    pub status: Option<u8>,
}

fn is_all_digits(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_digit())
}

fn is_upper_alpha3(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_valid_iban(value: &str) -> bool {
    value.len() == 29 && value.starts_with("UA") && value[2..].bytes().all(|b| b.is_ascii_digit())
}

impl NewCustomer {
    /// Structural checks (via `validator`) plus the kind-conditional and
    /// cross-field business rules from `docs/customers-crm.md` that don't
    /// reduce to a single-field derive attribute. Checksum validation of
    /// ЄДРПОУ/РНОКПП control digits is out of scope for the prototype
    /// (format-only).
    pub fn validate_domain(&self) -> Result<(), DomainError> {
        let mut details = match self.validate() {
            Ok(()) => HashMap::new(),
            Err(errors) => {
                let DomainError::Validation(details) = DomainError::from(errors) else {
                    unreachable!("From<ValidationErrors> always produces DomainError::Validation")
                };
                details
            }
        };

        if let Some(edrpou) = &self.edrpou {
            if self.kind != CustomerKind::LegalEntity {
                details
                    .entry("edrpou".to_string())
                    .or_insert_with(|| FieldError::code("not_applicable_for_kind"));
            } else if !is_all_digits(edrpou, 8) {
                details
                    .entry("edrpou".to_string())
                    .or_insert_with(|| FieldError::code("invalid_edrpou"));
            }
        }

        if let Some(tax_id) = &self.tax_id {
            if self.kind == CustomerKind::LegalEntity {
                details
                    .entry("tax_id".to_string())
                    .or_insert_with(|| FieldError::code("not_applicable_for_kind"));
            } else if !is_all_digits(tax_id, 10) {
                details
                    .entry("tax_id".to_string())
                    .or_insert_with(|| FieldError::code("invalid_tax_id"));
            }
        }

        if let Some(vat_ipn) = &self.vat_ipn {
            if !is_all_digits(vat_ipn, 12) {
                details
                    .entry("vat_ipn".to_string())
                    .or_insert_with(|| FieldError::code("invalid_vat_ipn"));
            }
        }

        if let Some(iban) = &self.iban {
            if !is_valid_iban(iban) {
                details
                    .entry("iban".to_string())
                    .or_insert_with(|| FieldError::code("invalid_iban"));
            }
        }

        if self.contacts.iter().filter(|c| c.is_primary).count() > 1 {
            details
                .entry("contacts".to_string())
                .or_insert_with(|| FieldError::code("multiple_primary_contacts"));
        }

        if let Some(currency) = &self.default_currency {
            if !is_upper_alpha3(currency) {
                details
                    .entry("default_currency".to_string())
                    .or_insert_with(|| FieldError::code("invalid_currency_code"));
            }
        }

        if let Some(credit_limit) = &self.credit_limit {
            if !is_upper_alpha3(&credit_limit.currency) {
                details
                    .entry("credit_limit.currency".to_string())
                    .or_insert_with(|| FieldError::code("invalid_currency_code"));
            }
        }

        if details.is_empty() {
            Ok(())
        } else {
            Err(DomainError::Validation(details))
        }
    }

    /// Creation-only: `status` may be absent (defaults to `active`) or `0`
    /// (`lead`); anything else is rejected. Not part of `validate_domain`
    /// because the same field is silently ignored on update.
    pub fn validate_creation_status(&self) -> Result<(), DomainError> {
        match self.status {
            None | Some(0) | Some(1) => Ok(()),
            Some(_) => {
                let mut details = HashMap::new();
                details.insert(
                    "status".to_string(),
                    FieldError::code("invalid_creation_status"),
                );
                Err(DomainError::Validation(details))
            }
        }
    }

    /// Trims, lowercases, drops empties, and dedupes tags. Not a validation
    /// step — normalization always succeeds — so the service calls this
    /// before `validate_domain`/persisting rather than folding it in there.
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.tags = std::mem::take(&mut self.tags)
            .into_iter()
            .map(|tag| tag.trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .filter(|tag| seen.insert(tag.clone()))
            .collect();
    }

    /// Fills `default_currency` from the tenant's default when the client
    /// omitted it, mirroring `NewOrder::resolve_currency`.
    pub fn resolve_default_currency(&mut self, tenant_default_currency: &str) {
        if self.default_currency.is_none() {
            self.default_currency = Some(tenant_default_currency.to_string());
        }
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
    pub status: Option<CustomerStatus>,
    /// Exact match against the normalized (trimmed, lowercased) tag.
    pub tag: Option<String>,
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
    /// `tenant` fills a missing `default_currency` on legacy rows (see
    /// `docs/customers-crm.md`'s migration notes) — every method that returns
    /// a `Customer` needs it since the repair happens at read time, not via
    /// backfill.
    async fn list(&self, query: ListQuery, tenant: &Tenant)
    -> Result<Paged<Customer>, DomainError>;
    async fn get(&self, id: &str, tenant: &Tenant) -> Result<Option<Customer>, DomainError>;
    /// `tenant` also supplies `customer_prefix` for the assigned number.
    async fn create(&self, data: NewCustomer, tenant: &Tenant) -> Result<Customer, DomainError>;
    async fn update(
        &self,
        id: &str,
        data: NewCustomer,
        tenant: &Tenant,
    ) -> Result<Customer, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
    async fn set_status(
        &self,
        id: &str,
        status: CustomerStatus,
        tenant: &Tenant,
    ) -> Result<Customer, DomainError>;
    /// Top BM25-ranked hits for the global omnibox (M3). `q` is assumed
    /// non-empty — callers filter that out before calling.
    async fn search(&self, q: &str, limit: u32) -> Result<Vec<crate::SearchHit>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_contact(is_primary: bool) -> Contact {
        Contact {
            name: "Ada Lovelace".to_string(),
            role: Some("директор".to_string()),
            email: Some("ada@adamant-print.example".to_string()),
            phone: None,
            is_primary,
        }
    }

    fn base_customer(kind: CustomerKind) -> NewCustomer {
        NewCustomer {
            kind,
            name: "Друкарня «Аркуш»".to_string(),
            legal_name: None,
            edrpou: None,
            tax_id: None,
            vat_ipn: None,
            tags: vec![],
            industry: None,
            source: None,
            website: None,
            contacts: vec![],
            legal_address: None,
            delivery_address: None,
            payment_terms_days: 0,
            credit_limit: None,
            default_currency: None,
            default_discount_bp: 0,
            iban: None,
            bank_name: None,
            notes: None,
            status: None,
        }
    }

    #[test]
    fn empty_name_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.name = String::new();

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("name"));
    }

    #[test]
    fn valid_legal_entity_passes() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.edrpou = Some("12345678".to_string());
        assert!(data.validate_domain().is_ok());
    }

    #[test]
    fn edrpou_rejected_for_fop() {
        let mut data = base_customer(CustomerKind::Fop);
        data.edrpou = Some("12345678".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["edrpou"].code, "not_applicable_for_kind");
    }

    #[test]
    fn malformed_edrpou_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.edrpou = Some("1234".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["edrpou"].code, "invalid_edrpou");
    }

    #[test]
    fn tax_id_rejected_for_legal_entity() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.tax_id = Some("1234567890".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["tax_id"].code, "not_applicable_for_kind");
    }

    #[test]
    fn valid_tax_id_for_fop_and_individual() {
        let mut fop = base_customer(CustomerKind::Fop);
        fop.tax_id = Some("1234567890".to_string());
        assert!(fop.validate_domain().is_ok());

        let mut individual = base_customer(CustomerKind::Individual);
        individual.tax_id = Some("1234567890".to_string());
        assert!(individual.validate_domain().is_ok());
    }

    #[test]
    fn vat_ipn_valid_for_any_kind() {
        let mut fop = base_customer(CustomerKind::Fop);
        fop.vat_ipn = Some("123456789012".to_string());
        assert!(fop.validate_domain().is_ok());
    }

    #[test]
    fn malformed_vat_ipn_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.vat_ipn = Some("123".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["vat_ipn"].code, "invalid_vat_ipn");
    }

    #[test]
    fn valid_iban_passes() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.iban = Some(format!("UA{}", "1".repeat(27)));
        assert!(data.validate_domain().is_ok());
    }

    #[test]
    fn malformed_iban_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.iban = Some("DE89370400440532013000".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["iban"].code, "invalid_iban");
    }

    #[test]
    fn multiple_primary_contacts_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.contacts = vec![valid_contact(true), valid_contact(true)];

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["contacts"].code, "multiple_primary_contacts");
    }

    #[test]
    fn single_primary_contact_is_valid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.contacts = vec![valid_contact(true), valid_contact(false)];
        assert!(data.validate_domain().is_ok());
    }

    #[test]
    fn contact_with_empty_name_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        let mut contact = valid_contact(false);
        contact.name = String::new();
        data.contacts = vec![contact];

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("contacts[0].name"));
    }

    #[test]
    fn contact_with_malformed_email_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        let mut contact = valid_contact(false);
        contact.email = Some("not-an-email".to_string());
        data.contacts = vec![contact];

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("contacts[0].email"));
    }

    #[test]
    fn short_country_code_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.legal_address = Some(Address {
            street: None,
            zip: None,
            city: None,
            country: Some("Germany".to_string()),
        });

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("legal_address.country"));
    }

    #[test]
    fn payment_terms_days_out_of_range_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.payment_terms_days = 400;

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("payment_terms_days"));
    }

    #[test]
    fn default_discount_bp_out_of_range_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.default_discount_bp = 10001;

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("default_discount_bp"));
    }

    #[test]
    fn negative_credit_limit_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.credit_limit = Some(Money {
            amount_minor: -1,
            currency: "UAH".to_string(),
        });

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("credit_limit.amount_minor"));
    }

    #[test]
    fn lowercase_default_currency_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.default_currency = Some("uah".to_string());

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["default_currency"].code, "invalid_currency_code");
    }

    #[test]
    fn lowercase_credit_limit_currency_is_invalid() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.credit_limit = Some(Money {
            amount_minor: 100,
            currency: "uah".to_string(),
        });

        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(
            details["credit_limit.currency"].code,
            "invalid_currency_code"
        );
    }

    #[test]
    fn creation_status_accepts_none_lead_or_active() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        assert!(data.validate_creation_status().is_ok());
        data.status = Some(0);
        assert!(data.validate_creation_status().is_ok());
        data.status = Some(1);
        assert!(data.validate_creation_status().is_ok());
    }

    #[test]
    fn creation_status_rejects_anything_else() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.status = Some(2);
        let err = data.validate_creation_status().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert_eq!(details["status"].code, "invalid_creation_status");
    }

    #[test]
    fn normalize_trims_lowercases_drops_empties_and_dedupes() {
        let mut data = base_customer(CustomerKind::LegalEntity);
        data.tags = vec![
            "  Опт ".to_string(),
            "опт".to_string(),
            "".to_string(),
            "  ".to_string(),
            "Постійний".to_string(),
        ];
        data.normalize();
        assert_eq!(data.tags, vec!["опт".to_string(), "постійний".to_string()]);
    }

    #[test]
    fn resolve_default_currency_fills_only_when_absent() {
        let mut with_default = base_customer(CustomerKind::LegalEntity);
        with_default.resolve_default_currency("UAH");
        assert_eq!(with_default.default_currency.as_deref(), Some("UAH"));

        let mut with_explicit = base_customer(CustomerKind::LegalEntity);
        with_explicit.default_currency = Some("USD".to_string());
        with_explicit.resolve_default_currency("UAH");
        assert_eq!(with_explicit.default_currency.as_deref(), Some("USD"));
    }

    #[test]
    fn can_order_only_for_lead_and_active() {
        assert!(can_order(CustomerStatus::Lead));
        assert!(can_order(CustomerStatus::Active));
        assert!(!can_order(CustomerStatus::Inactive));
        assert!(!can_order(CustomerStatus::Blocked));
    }

    #[test]
    fn transition_matrix_matches_lifecycle_diagram() {
        use CustomerStatus::*;
        let statuses = [Lead, Active, Inactive, Blocked];
        let allowed: &[(CustomerStatus, CustomerStatus)] = &[
            (Lead, Active),
            (Active, Inactive),
            (Active, Blocked),
            (Inactive, Active),
            (Inactive, Blocked),
            (Blocked, Active),
        ];
        for &from in &statuses {
            for &to in &statuses {
                let expected = allowed.contains(&(from, to));
                assert_eq!(
                    can_transition(from, to),
                    expected,
                    "{from:?} -> {to:?} expected {expected}"
                );
                assert_eq!(validate_transition(from, to).is_ok(), expected);
            }
        }
    }
}
