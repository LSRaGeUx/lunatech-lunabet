use std::net::IpAddr;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;

use crate::state::AppState;
use crate::tenant::UnknownSlug;

#[derive(Debug, Deserialize)]
struct TenantQuery {
    tenant: Option<String>,
}

/// What the request "wants" in terms of tenant resolution.
enum SlugIntent {
    /// Header / query / subdomain says: route to this slug.
    Slug(String),
    /// The host is the platform apex (or a bare IP / localhost); no tenant
    /// scope, route to marketing pages.
    Apex,
    /// Neither apex nor a tenant subdomain: fall back to the deployment's
    /// default tenant. Keeps backwards compatibility with single-tenant
    /// hosts (the Clever Cloud free `*.cleverapps.io` URL while DNS is being
    /// set up).
    Default,
}

fn classify(req: &Request<Body>, apex_hosts: &std::collections::HashSet<String>) -> SlugIntent {
    if let Some(h) = req.headers().get("X-Tenant").and_then(|v| v.to_str().ok()) {
        let t = h.trim();
        if !t.is_empty() {
            return SlugIntent::Slug(t.to_string());
        }
    }
    if let Ok(Query(q)) = Query::<TenantQuery>::try_from_uri(req.uri()) {
        if let Some(t) = q.tenant {
            let t = t.trim();
            if !t.is_empty() {
                return SlugIntent::Slug(t.to_string());
            }
        }
    }

    let host = req
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let host_only = host.split(':').next().unwrap_or(host).to_lowercase();

    if host_only.is_empty() {
        return SlugIntent::Default;
    }
    if apex_hosts.contains(&host_only) {
        return SlugIntent::Apex;
    }
    // Bare IPv4/IPv6 → fall back to the default tenant. Treating them as
    // apex would break local dev (curl on 127.0.0.1) and any deployment
    // accessed directly by IP rather than DNS name.
    if host_only.parse::<IpAddr>().is_ok() {
        return SlugIntent::Default;
    }
    // Two-or-more-dot host where the first label isn't "www" → tenant.
    if host_only.matches('.').count() >= 2 {
        if let Some(first) = host_only.split('.').next() {
            if !first.is_empty() && first != "www" {
                return SlugIntent::Slug(first.to_string());
            }
        }
    }
    SlugIntent::Default
}

/// Resolve the tenant for the current request.
///
/// Attaches a `Tenant` to request extensions when the request targets one
/// (subdomain, header, query, or single-tenant fallback). When the request
/// targets the platform apex (the marketing host) no tenant is attached, and
/// downstream handlers must use the `MaybeTenant` extractor (or the
/// `TenantCtx` extractor will redirect to `/`).
pub async fn resolve_tenant(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let intent = classify(&req, &state.cfg.apex_hosts);
    match intent {
        SlugIntent::Apex => {
            // No tenant attached: handlers using MaybeTenant render the
            // platform marketing pages, TenantCtx redirects to "/".
        }
        SlugIntent::Default => {
            // Resolve through the cache (not the frozen boot snapshot) so admin
            // edits to the default tenant take effect without a restart.
            req.extensions_mut()
                .insert(state.tenants.resolve_default().await);
        }
        SlugIntent::Slug(s) => match state.tenants.resolve(&s).await {
            Some(t) => {
                req.extensions_mut().insert(t);
            }
            None => {
                req.extensions_mut().insert(UnknownSlug(s));
            }
        },
    }
    next.run(req).await
}
