/// Resolved from a verified JWT by the auth middleware: the caller's identity
/// and the tenant database their org claim maps to.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub org_id: String,
    pub tenant_db: String,
}
