use serde::Serialize;

/// One ranked hit for the global omnibox (`GET /api/search`), per PLAN.md's
/// response shape: `{id, label, highlight}`. `highlight` falls back to the
/// plain `label` when the query matched a different field than the one
/// shown as the label (e.g. a customer's email, not its name).
#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub label: String,
    pub highlight: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SearchResults {
    pub customers: Vec<SearchHit>,
    pub orders: Vec<SearchHit>,
    pub invoices: Vec<SearchHit>,
    pub quotes: Vec<SearchHit>,
}
