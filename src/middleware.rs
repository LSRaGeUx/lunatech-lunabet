use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct TenantQuery {
    tenant: Option<String>,
}

/// Best-effort extraction of a tenant slug from the incoming request.
///
/// Precedence:
///   1. The `X-Tenant` header (handy for local dev / curl / tests).
///   2. The `?tenant=<slug>` query parameter (handy for browser testing
///      before DNS is set up).
///   3. The first label of the `Host` header, when the host has at least two
///      dots (so `acme.lunabet.eu` resolves to `acme`, while `localhost` or
///      `lunabet.eu` resolve to nothing).
fn extract_slug(req: &Request<Body>) -> Option<String> {
    if let Some(h) = req.headers().get("X-Tenant").and_then(|v| v.to_str().ok()) {
        let t = h.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    if let Ok(Query(q)) = Query::<TenantQuery>::try_from_uri(req.uri()) {
        if let Some(t) = q.tenant {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    if let Some(host) = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
    {
        // Strip the optional ":port" suffix.
        let host_only = host.split(':').next().unwrap_or(host);
        // Require at least one dot beyond the first label, otherwise this is
        // a bare hostname like `localhost` or an apex domain like
        // `lunabet.eu` with no tenant subdomain.
        if host_only.matches('.').count() >= 2 {
            if let Some(first) = host_only.split('.').next() {
                if !first.is_empty() && first != "www" {
                    return Some(first.to_string());
                }
            }
        }
    }

    None
}

/// Resolve the tenant for the current request and stash it in the request
/// extensions for downstream handlers to read via the `TenantCtx` extractor.
/// When no tenant can be resolved, fall back to the registry's default tenant
/// so that bare `localhost` and pre-DNS deployments keep working.
pub async fn resolve_tenant(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let slug = extract_slug(&req);
    let tenant = match slug {
        Some(s) => state
            .tenants
            .resolve(&s)
            .await
            .unwrap_or_else(|| state.tenants.default_tenant().clone()),
        None => state.tenants.default_tenant().clone(),
    };
    req.extensions_mut().insert(tenant);
    next.run(req).await
}
