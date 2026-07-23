//! Typed live-change streams over the three tenant tables. The api crate's
//! WS hub consumes these; it never sees `RecordId` or the row structs — the
//! same layering rule as the repos.

use std::sync::Arc;

use domain::error::DomainError;
use domain::{ChangeAction, ChangeEvent, Invoice, LiveChange, Order};
use futures::stream::{Stream, StreamExt, select_all};
use surrealdb::engine::any::Any;
use surrealdb::types::Action;
use surrealdb::{Notification, Surreal};

use crate::common::map_err;
use crate::customer_repo::{CustomerRow, customer_from_row_untenanted};
use crate::invoice_repo::InvoiceRow;
use crate::order_repo::OrderRow;
use crate::{customer_repo, invoice_repo, order_repo};

fn change_action(action: Action) -> Result<ChangeAction, DomainError> {
    match action {
        Action::Create => Ok(ChangeAction::Create),
        Action::Update => Ok(ChangeAction::Update),
        Action::Delete => Ok(ChangeAction::Delete),
        // `Killed` terminates the SDK stream before an item is produced, so
        // it can't reach here; guard anyway because `Action` is
        // non-exhaustive.
        other => Err(DomainError::Store(format!(
            "unexpected live-query action: {other:?}"
        ))),
    }
}

fn map_event<Row, T>(
    notification: surrealdb::Result<Notification<Row>>,
    key: impl Fn(&Row) -> String,
    convert: impl Fn(Row) -> Result<T, DomainError>,
) -> Result<ChangeEvent<T>, DomainError> {
    let notification = notification.map_err(map_err)?;
    let action = change_action(notification.action)?;
    let id = key(&notification.data);
    // Delete notifications carry the deleted record's content; the protocol
    // sends `data: null` instead, so drop it here.
    let data = match action {
        ChangeAction::Delete => None,
        _ => Some(convert(notification.data)?),
    };
    Ok(ChangeEvent { action, id, data })
}

/// A merged live-change stream that keeps its session alive.
///
/// Owning the session is load-bearing, not a convenience: `Surreal::drop`
/// sends a `detach` RPC that destroys the server-side session, and live
/// notifications are tagged with — and routed via — the session that
/// registered the query. If the last `Arc` to the session dropped while
/// this stream was still open, every notification would be silently
/// discarded (no error, the stream just never yields). See
/// docs/adr/0008-live-stream-session-lifetime.md.
pub struct LiveChanges {
    inner: futures::stream::SelectAll<
        futures::stream::BoxStream<'static, Result<LiveChange, DomainError>>,
    >,
    _session: Arc<Surreal<Any>>,
}

impl Stream for LiveChanges {
    type Item = Result<LiveChange, DomainError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Opens `LIVE SELECT` streams on `customer`, `order`, and `invoice` in the
/// session's database and merges them into one stream of domain-typed
/// changes. Dropping the returned stream kills the live queries server-side
/// (the SDK sends `KILL` from each inner stream's `Drop`) — the hub's
/// teardown relies on this.
///
/// An `Err` item means a notification failed to arrive or convert; callers
/// should treat it like a broken stream (re-open and resync).
pub async fn live_changes(session: Arc<Surreal<Any>>) -> Result<LiveChanges, DomainError> {
    let customers = session
        .select::<Vec<CustomerRow>>(customer_repo::TABLE)
        .live()
        .await
        .map_err(map_err)?;
    let orders = session
        .select::<Vec<OrderRow>>(order_repo::TABLE)
        .live()
        .await
        .map_err(map_err)?;
    let invoices = session
        .select::<Vec<InvoiceRow>>(invoice_repo::TABLE)
        .live()
        .await
        .map_err(map_err)?;

    let customers = customers
        .map(|n| {
            map_event(n, CustomerRow::key, customer_from_row_untenanted)
                .map(|e| LiveChange::Customer(Box::new(e)))
        })
        .boxed();
    let orders = orders
        .map(|n| {
            map_event(n, OrderRow::key, Order::try_from).map(|e| LiveChange::Order(Box::new(e)))
        })
        .boxed();
    let invoices = invoices
        .map(|n| {
            map_event(n, InvoiceRow::key, Invoice::try_from)
                .map(|e| LiveChange::Invoice(Box::new(e)))
        })
        .boxed();

    Ok(LiveChanges {
        inner: select_all([customers, orders, invoices]),
        _session: session,
    })
}
