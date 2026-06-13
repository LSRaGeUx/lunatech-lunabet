use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use sqlx::PgPool;

use crate::config::Config;
use crate::rate_limit::{EndpointRateLimiter, SignupRateLimiter};
use crate::tenant::TenantRegistry;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cookie_key: Key,
    pub cfg: Config,
    pub http: reqwest::Client,
    /// Directory of all known tenants plus the deployment's default tenant.
    /// The resolution middleware reads this registry to attach the right
    /// tenant to each incoming request; background jobs use the default
    /// tenant until Phase 5 introduces per-tenant scheduling.
    pub tenants: TenantRegistry,
    /// Anti-abuse for `POST /signup`. Clones share state via Arc.
    pub signup_limiter: SignupRateLimiter,
    /// Rate limiter for other endpoints (e.g., /login).
    pub endpoint_limiter: EndpointRateLimiter,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}
