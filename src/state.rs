use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sqlx::PgPool;

use crate::config::Config;
use crate::tenant::Tenant;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cookie_key: Key,
    pub cfg: Config,
    pub http: reqwest::Client,
    /// The active tenant for the current deployment. While the app is
    /// single-tenant per process this is loaded once at startup; when we add
    /// per-request tenant resolution this will be moved into a request
    /// extension and removed from `AppState`.
    pub tenant: Tenant,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
