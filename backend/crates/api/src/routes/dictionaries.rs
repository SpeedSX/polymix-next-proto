use std::collections::HashMap;

use axum::Json;
use domain::customer::{CustomerStatus, can_order};
use domain::order::{OrderStatus, can_invoice};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OrderStatusDictionaryItem {
    pub id: u8,
    pub key: &'static str,
    pub sort: u8,
    pub color: &'static str,
    pub invoiceable: bool,
    pub allowed_targets: Vec<u8>,
    pub labels: HashMap<&'static str, &'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderStatusDictionaryResponse {
    pub items: Vec<OrderStatusDictionaryItem>,
}

fn allowed_targets(status: OrderStatus) -> Vec<OrderStatus> {
    use OrderStatus::*;
    match status {
        Draft => vec![Confirmed, Cancelled],
        Confirmed => vec![InProduction, Cancelled],
        InProduction => vec![Completed],
        Completed | Cancelled => vec![],
    }
}

fn color(status: OrderStatus) -> &'static str {
    use OrderStatus::*;
    match status {
        Draft => "gray",
        Confirmed => "blue",
        InProduction => "orange",
        Completed => "green",
        Cancelled => "red",
    }
}

fn labels(status: OrderStatus) -> HashMap<&'static str, &'static str> {
    use OrderStatus::*;
    // NOTE: These labels intentionally mirror the current frontend i18n
    // translations (en/ua) so the UI can switch languages without needing
    // to ship its own status label tables.
    match status {
        Draft => HashMap::from([("en", "Draft"), ("ua", "Чернетка")]),
        Confirmed => HashMap::from([("en", "Confirmed"), ("ua", "Підтверджено")]),
        InProduction => HashMap::from([("en", "In production"), ("ua", "У виробництві")]),
        Completed => HashMap::from([("en", "Completed"), ("ua", "Завершено")]),
        Cancelled => HashMap::from([("en", "Cancelled"), ("ua", "Скасовано")]),
    }
}

pub async fn order_statuses() -> Json<OrderStatusDictionaryResponse> {
    let all = [
        OrderStatus::Draft,
        OrderStatus::Confirmed,
        OrderStatus::InProduction,
        OrderStatus::Completed,
        OrderStatus::Cancelled,
    ];

    let items = all
        .into_iter()
        .map(|status| OrderStatusDictionaryItem {
            id: status.code(),
            key: status.key(),
            sort: status.code(),
            color: color(status),
            invoiceable: can_invoice(status),
            allowed_targets: allowed_targets(status)
                .into_iter()
                .map(OrderStatus::code)
                .collect(),
            labels: labels(status),
        })
        .collect();

    Json(OrderStatusDictionaryResponse { items })
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomerStatusDictionaryItem {
    pub id: u8,
    pub key: &'static str,
    pub sort: u8,
    pub color: &'static str,
    pub can_order: bool,
    pub allowed_targets: Vec<u8>,
    pub labels: HashMap<&'static str, &'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomerStatusDictionaryResponse {
    pub items: Vec<CustomerStatusDictionaryItem>,
}

fn customer_allowed_targets(status: CustomerStatus) -> Vec<CustomerStatus> {
    use CustomerStatus::*;
    match status {
        Lead => vec![Active],
        Active => vec![Inactive, Blocked],
        Inactive => vec![Active, Blocked],
        Blocked => vec![Active],
    }
}

fn customer_color(status: CustomerStatus) -> &'static str {
    use CustomerStatus::*;
    match status {
        Lead => "gray",
        Active => "green",
        Inactive => "yellow",
        Blocked => "red",
    }
}

fn customer_labels(status: CustomerStatus) -> HashMap<&'static str, &'static str> {
    use CustomerStatus::*;
    // NOTE: mirrors the current frontend i18n translations (en/ua), same
    // rationale as `labels` above.
    match status {
        Lead => HashMap::from([("en", "Lead"), ("ua", "Лід")]),
        Active => HashMap::from([("en", "Active"), ("ua", "Активний")]),
        Inactive => HashMap::from([("en", "Inactive"), ("ua", "Неактивний")]),
        Blocked => HashMap::from([("en", "Blocked"), ("ua", "Заблокований")]),
    }
}

pub async fn customer_statuses() -> Json<CustomerStatusDictionaryResponse> {
    let all = [
        CustomerStatus::Lead,
        CustomerStatus::Active,
        CustomerStatus::Inactive,
        CustomerStatus::Blocked,
    ];

    let items = all
        .into_iter()
        .map(|status| CustomerStatusDictionaryItem {
            id: status.code(),
            key: status.key(),
            sort: status.code(),
            color: customer_color(status),
            can_order: can_order(status),
            allowed_targets: customer_allowed_targets(status)
                .into_iter()
                .map(CustomerStatus::code)
                .collect(),
            labels: customer_labels(status),
        })
        .collect();

    Json(CustomerStatusDictionaryResponse { items })
}
