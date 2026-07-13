use std::collections::HashMap;

use axum::Json;
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

