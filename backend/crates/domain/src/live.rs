use crate::{Customer, Invoice, Order, Quote};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeAction {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
pub struct ChangeEvent<T> {
    pub action: ChangeAction,
    pub id: String,
    /// `Some(entity)` for create/update, `None` for delete.
    pub data: Option<T>,
}

#[derive(Debug, Clone)]
pub enum LiveChange {
    Customer(Box<ChangeEvent<Customer>>),
    Order(Box<ChangeEvent<Order>>),
    Invoice(Box<ChangeEvent<Invoice>>),
    Quote(Box<ChangeEvent<Quote>>),
}
