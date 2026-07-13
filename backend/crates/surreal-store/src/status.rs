use domain::error::DomainError;
use domain::order::OrderStatus;

pub(crate) fn order_status_from_db(value: i64) -> Result<OrderStatus, DomainError> {
    let code: u8 = value.try_into().map_err(|_| {
        DomainError::Store(format!("invalid order status code (out of range): {value}"))
    })?;
    OrderStatus::from_code(code)
        .ok_or_else(|| DomainError::Store(format!("unknown order status code: {value}")))
}

