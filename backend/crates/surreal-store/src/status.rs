use domain::customer::{CustomerKind, CustomerStatus};
use domain::error::DomainError;
use domain::order::OrderStatus;

pub(crate) fn order_status_from_db(value: i64) -> Result<OrderStatus, DomainError> {
    let code: u8 = value.try_into().map_err(|_| {
        DomainError::Store(format!("invalid order status code (out of range): {value}"))
    })?;
    OrderStatus::from_code(code)
        .ok_or_else(|| DomainError::Store(format!("unknown order status code: {value}")))
}

pub(crate) fn customer_kind_from_db(value: i64) -> Result<CustomerKind, DomainError> {
    let code: u8 = value.try_into().map_err(|_| {
        DomainError::Store(format!(
            "invalid customer kind code (out of range): {value}"
        ))
    })?;
    CustomerKind::from_code(code)
        .ok_or_else(|| DomainError::Store(format!("unknown customer kind code: {value}")))
}

pub(crate) fn customer_status_from_db(value: i64) -> Result<CustomerStatus, DomainError> {
    let code: u8 = value.try_into().map_err(|_| {
        DomainError::Store(format!(
            "invalid customer status code (out of range): {value}"
        ))
    })?;
    CustomerStatus::from_code(code)
        .ok_or_else(|| DomainError::Store(format!("unknown customer status code: {value}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_message(err: DomainError) -> String {
        match err {
            DomainError::Store(message) => message,
            other => panic!("expected a store error, got {other:?}"),
        }
    }

    #[test]
    fn order_status_round_trips_every_known_code() {
        for status in [
            OrderStatus::Draft,
            OrderStatus::Confirmed,
            OrderStatus::InProduction,
            OrderStatus::Completed,
            OrderStatus::Cancelled,
        ] {
            assert_eq!(order_status_from_db(status.code() as i64).unwrap(), status);
        }
    }

    #[test]
    fn order_status_rejects_unknown_code() {
        let message = store_message(order_status_from_db(99).unwrap_err());
        assert!(
            message.contains("unknown order status code: 99"),
            "{message}"
        );
    }

    #[test]
    fn order_status_rejects_out_of_range_code() {
        // Larger than u8::MAX, so the `try_into::<u8>` fails before the
        // enum lookup — a distinct error message from the "unknown" case.
        let message = store_message(order_status_from_db(300).unwrap_err());
        assert!(message.contains("out of range"), "{message}");

        let negative = store_message(order_status_from_db(-1).unwrap_err());
        assert!(negative.contains("out of range"), "{negative}");
    }

    #[test]
    fn customer_kind_round_trips_every_known_code() {
        for kind in [
            CustomerKind::LegalEntity,
            CustomerKind::Fop,
            CustomerKind::Individual,
        ] {
            assert_eq!(customer_kind_from_db(kind.code() as i64).unwrap(), kind);
        }
    }

    #[test]
    fn customer_kind_rejects_unknown_and_out_of_range_codes() {
        assert!(
            store_message(customer_kind_from_db(9).unwrap_err())
                .contains("unknown customer kind code: 9")
        );
        assert!(store_message(customer_kind_from_db(1000).unwrap_err()).contains("out of range"));
    }

    #[test]
    fn customer_status_round_trips_every_known_code() {
        for status in [
            CustomerStatus::Lead,
            CustomerStatus::Active,
            CustomerStatus::Inactive,
            CustomerStatus::Blocked,
        ] {
            assert_eq!(
                customer_status_from_db(status.code() as i64).unwrap(),
                status
            );
        }
    }

    #[test]
    fn customer_status_rejects_unknown_and_out_of_range_codes() {
        assert!(
            store_message(customer_status_from_db(7).unwrap_err())
                .contains("unknown customer status code: 7")
        );
        assert!(store_message(customer_status_from_db(-5).unwrap_err()).contains("out of range"));
    }
}
